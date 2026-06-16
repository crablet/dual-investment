//! 接货备资按现货、期权可用余款、活期理财的顺序推进。
//! 每次资金动作先保存计划，再确认原请求和实际到账；收尾归集另行处理。

use super::funding::{
    settlement_funding_amount, settlement_option_funding_amount, spot_has_settlement_assets,
};
use super::{
    AppError, AppResult, AppState, BinanceClient, Decimal, FundingAttemptKind, SettlementBatch,
    SettlementStage, SpotAccountSnapshot, Uuid, round_down_funding_amount, spot_asset_free,
};
use crate::domain::{
    SettlementAssets, SettlementOptionFunding, SettlementRedemption, SimpleEarnSnapshot,
};

impl AppState {
    /// 调用者持有交易锁。先完成已登记步骤，再按最新余额决定下一步。
    pub(super) async fn prepare_batch_funding(
        &self,
        client: &BinanceClient,
        batch: &mut SettlementBatch,
    ) -> AppResult<bool> {
        if batch.option_funding.is_some() && batch.redemption.is_some() {
            return Err(AppError::Storage(crate::msg!(
                "backend.concurrentFundingPlansExistForThisBatch"
            )));
        }
        if !self.confirm_batch_option_funding(client, batch).await?
            || !self.confirm_batch_redemption(client, batch).await?
        {
            return Ok(false);
        }

        let mut transferred = false;
        loop {
            let spot = client.spot_account().await?;
            if !spot.can_trade {
                return Err(AppError::RiskRejected(crate::msg!(
                    "backend.spotAccountCannotTradeBatchSettlementStopped"
                )));
            }
            if spot_has_settlement_assets(&spot, &batch.required) {
                batch.shortfall = SettlementAssets::default();
                self.save_batch(batch)?;
                return Ok(true);
            }
            let option_amount =
                if !transferred && batch.required.usdt > spot_asset_free(&spot, "USDT") {
                    settlement_option_funding_amount(
                        &client.option_account().await?,
                        &spot,
                        &batch.required,
                    )
                } else {
                    Decimal::ZERO
                };
            let mut earn = SimpleEarnSnapshot {
                balances: Vec::new(),
            };
            for (asset, available) in [
                ("USDT", spot_asset_free(&spot, "USDT") + option_amount),
                ("BTC", spot_asset_free(&spot, "BTC")),
            ] {
                let required = if asset == "USDT" {
                    batch.required.usdt
                } else {
                    batch.required.btc
                };
                if required > available {
                    earn.balances
                        .extend(client.simple_earn_flexible_positions(asset).await?.balances);
                }
            }
            let plans = self.plan_batch_redemptions(batch, &spot, &earn, option_amount)?;
            if batch.shortfall.usdt > Decimal::ZERO || batch.shortfall.btc > Decimal::ZERO {
                self.update_batch(
                    batch,
                    SettlementStage::WaitingFunds,
                    &crate::msg!(
                        "backend.spotAvailableOptionsFundsAndFlexibleEarn",
                        p0 = batch.shortfall.usdt,
                        p1 = batch.shortfall.btc
                    ),
                )?;
                return Ok(false);
            }
            if option_amount > Decimal::ZERO {
                batch.option_funding = Some(SettlementOptionFunding {
                    chain_id: format!("{}-fund-option-{}", batch.id, Uuid::new_v4().simple()),
                    amount: option_amount,
                    expected_spot: spot_asset_free(&spot, "USDT") + option_amount,
                });
                self.update_batch(
                    batch,
                    SettlementStage::ConfirmingFunds,
                    &crate::msg!("backend.optionsSurplusTransferPlanRecordedWaitingFor"),
                )?;
                if !self.confirm_batch_option_funding(client, batch).await? {
                    return Ok(false);
                }
                // 到账后重算理财缺口，不能把计划中的划转金额当成现货余额。
                transferred = true;
                continue;
            }
            return self
                .start_batch_redemption(client, batch, &spot, plans)
                .await;
        }
    }

    async fn confirm_batch_option_funding(
        &self,
        client: &BinanceClient,
        batch: &mut SettlementBatch,
    ) -> AppResult<bool> {
        let Some(plan) = batch.option_funding.clone() else {
            return Ok(true);
        };
        let attempts = self.funding_attempts_by_chain(&plan.chain_id)?;
        if attempts.is_empty() {
            let account = client.option_account().await?;
            let spot = client.spot_account().await?;
            if !spot.can_trade {
                return Err(AppError::RiskRejected(crate::msg!(
                    "backend.spotAccountCannotTradeOptionsSettlementFunding"
                )));
            }
            if settlement_option_funding_amount(&account, &spot, &batch.required) < plan.amount {
                batch.option_funding = None;
                self.update_batch(
                    batch,
                    SettlementStage::ConfirmingFunds,
                    &crate::msg!("backend.balancesOrMarginRequirementsChangedRecalculatingThe"),
                )?;
                return Ok(false);
            }
        } else if attempts.len() != 1
            || attempts.iter().any(|attempt| {
                attempt.kind != FundingAttemptKind::UniversalTransfer
                    || attempt.asset != "USDT"
                    || attempt.transfer_type.as_deref() != Some("OPTION_MAIN")
                    || attempt.amount != plan.amount
            })
        {
            return Err(AppError::Storage(crate::msg!(
                "backend.optionsSettlementFundingPlanConflictsWithRecovery"
            )));
        }
        self.execute_universal_transfer(
            client,
            &plan.chain_id,
            &crate::msg!("backend.fundingForBatchExpirySettlement"),
            "OPTION_MAIN",
            "USDT",
            plan.amount,
        )
        .await?;
        self.update_batch(
            batch,
            SettlementStage::ConfirmingFunds,
            &crate::msg!("backend.optionsSurplusTransferConfirmedWaitingForSpot"),
        )?;
        if !self
            .wait_for_spot_asset_available(client, "USDT", plan.expected_spot)
            .await?
        {
            return Ok(false);
        }
        batch.option_funding = None;
        self.save_batch(batch)?;
        Ok(true)
    }

    async fn confirm_batch_redemption(
        &self,
        client: &BinanceClient,
        batch: &mut SettlementBatch,
    ) -> AppResult<bool> {
        let Some(plan) = batch.redemption.clone() else {
            return Ok(true);
        };
        self.execute_simple_earn_redeem(
            client,
            &plan.chain_id,
            &crate::msg!("backend.spotRebalancingForBatchExpiry"),
            &plan.product_id,
            &plan.asset,
            plan.amount,
        )
        .await?;
        self.update_batch(
            batch,
            SettlementStage::ConfirmingFunds,
            &crate::msg!("backend.flexibleEarnRedemptionConfirmedWaitingForSpot"),
        )?;
        if !self
            .wait_for_spot_asset_available(client, &plan.asset, plan.expected_spot)
            .await?
        {
            return Ok(false);
        }
        batch.redemption = None;
        self.save_batch(batch)?;
        Ok(true)
    }

    async fn start_batch_redemption(
        &self,
        client: &BinanceClient,
        batch: &mut SettlementBatch,
        spot: &SpotAccountSnapshot,
        plans: Vec<SettlementRedemption>,
    ) -> AppResult<bool> {
        let Some(plan) = plans.into_iter().next() else {
            batch.shortfall = SettlementAssets {
                usdt: settlement_funding_amount(
                    "USDT",
                    batch.required.usdt - spot_asset_free(spot, "USDT"),
                ),
                btc: settlement_funding_amount(
                    "BTC",
                    batch.required.btc - spot_asset_free(spot, "BTC"),
                ),
            };
            self.update_batch(
                batch,
                SettlementStage::WaitingFunds,
                &crate::msg!(
                    "backend.aRecordedRedemptionCannotBeResentSpot",
                    p0 = batch.shortfall.usdt,
                    p1 = batch.shortfall.btc
                ),
            )?;
            return Ok(false);
        };
        batch.redemption = Some(plan);
        self.update_batch(
            batch,
            SettlementStage::ConfirmingFunds,
            &crate::msg!("backend.flexibleEarnRedemptionPlanRecordedWaitingFor"),
        )?;
        if !self.confirm_batch_redemption(client, batch).await? {
            return Ok(false);
        }
        Ok(spot_has_settlement_assets(
            &client.spot_account().await?,
            &batch.required,
        ))
    }

    fn plan_batch_redemptions(
        &self,
        batch: &mut SettlementBatch,
        spot: &SpotAccountSnapshot,
        earn: &SimpleEarnSnapshot,
        option_usdt: Decimal,
    ) -> AppResult<Vec<SettlementRedemption>> {
        let needs = [("USDT", batch.required.usdt), ("BTC", batch.required.btc)];
        let mut plans = Vec::new();
        batch.shortfall = SettlementAssets::default();
        for (asset, required) in needs {
            let free = spot_asset_free(spot, asset);
            let from_option = if asset == "USDT" {
                option_usdt
            } else {
                Decimal::ZERO
            };
            let deficit = settlement_funding_amount(asset, required - free - from_option);
            let eligible = earn.balances.iter().filter(|row| {
                row.asset == asset && row.flexible && row.can_redeem && !row.product_id.is_empty()
            });
            let total = eligible.clone().map(|row| row.amount).sum::<Decimal>();
            let shortfall = (deficit - total).max(Decimal::ZERO);
            if asset == "USDT" {
                batch.shortfall.usdt = shortfall;
            } else {
                batch.shortfall.btc = shortfall;
            }
            // 先划回期权余款；赎回计划只能基于划转后的真实现货余额保存。
            if deficit <= Decimal::ZERO || option_usdt > Decimal::ZERO {
                continue;
            }
            let mut positions = eligible.collect::<Vec<_>>();
            positions.sort_by_key(|position| std::cmp::Reverse(position.amount));
            for position in positions {
                let chain_id = format!("{}-redeem-{asset}-{}", batch.id, position.product_id);
                if !self.funding_attempts_by_chain(&chain_id)?.is_empty() {
                    continue;
                }
                let amount = round_down_funding_amount(position.amount.min(deficit));
                if amount > Decimal::ZERO {
                    plans.push(SettlementRedemption {
                        chain_id,
                        asset: asset.to_string(),
                        product_id: position.product_id.clone(),
                        amount,
                        expected_spot: free + amount,
                    });
                    break;
                }
            }
        }
        Ok(plans)
    }
}
