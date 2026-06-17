# 构建与发布安装包

[返回项目首页](../README.md) · [开发指南](development.md) · [构建工作流](../.github/workflows/build-desktop.yml)

仓库提供 `Build desktop installers` 工作流，在版本标签推送或手动触发后构建安装包，上传到 **GitHub Releases 草稿**。发布由维护者手动完成。

## 平台与产物

| 平台 | 标准 GitHub runner | 产物 |
| --- | --- | --- |
| Linux x64 | `ubuntu-24.04` | `.deb`、`.rpm` |
| Windows x64 | `windows-2025` | NSIS `-setup.exe` |
| macOS 13.3+，Apple Silicon + Intel | `macos-15` | Universal `.dmg`，包含 `.app` 应用包 |

macOS 在一个任务内构建两种架构并合并为通用应用。Windows 不使用付费签名服务；macOS 使用 ad-hoc 签名，不接入付费 Apple Developer 签名或公证。

工作流固定第三方 Actions 的提交版本、pnpm 12.9.1 和 Rust 1.99.1，使用 Node.js 24。前端以 `--frozen-lockfile` 安装，Rust 构建使用 `--locked`；构建中执行前端 lint、Rust 格式检查以及 Tauri 自带的前端类型检查和原生编译。流水线不启动客户端，也不需要 Binance 凭据。

## 免费运行的边界

按 [GitHub 当前计费规则](https://docs.github.com/en/billing/concepts/product-billing/github-actions)，公开仓库的标准 GitHub-hosted runner 免费；私有仓库存在用量额度和超额收费，较大规格 runner 也可能收费。

本工作流通过以下配置限制费用来源：

- **所有 job 在分配 runner 前检查仓库必须公开**，私有或内部仓库全部跳过；手动触发也不能绕过。这里指 GitHub 仓库的可见性，与 `package.json` 中的 `private` 字段无关。
- 仅使用表中的三种[标准 runner](https://docs.github.com/en/actions/reference/runners/github-hosted-runners#standard-github-hosted-runners-for-public-repositories)，不使用 larger runner、自建付费机器或外部构建服务。
- 关闭 pnpm、Node.js 的 Actions 缓存，不配置 Rust 缓存，不上传 Actions artifacts。安装包直接作为 Releases 附件保存；[GitHub Releases 文档](https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases#storage-and-bandwidth-quotas)说明单个附件须小于 2 GiB，总大小和下载带宽不设配额限制。
- 只在版本标签或手动触发时构建，不对普通提交、PR 或定时任务自动运行；设置任务超时，同一标签的构建依次进行。

这些限制适用于本工作流及当前 GitHub 政策，不会更改账户账单设置，也不约束仓库内其他工作流。修改仓库可见性、runner、缓存、上传方式或引入其他服务前，应重新核对费用规则。

## 发布一个版本

1. 在 GitHub 确认仓库已公开，并允许 GitHub Actions 运行。私有状态下工作流会显示跳过，不会占用 runner。
2. 同步 `package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml` 的版本号，并更新 `Cargo.lock` 中的本地包版本。按[开发指南](development.md#验证与打包)完成本地检查。
3. 将代码和工作流提交、推送到仓库。为该提交创建与版本一致的新标签并推送，例如当前 `0.1.0` 对应：

   ```bash
   git tag v0.1.0
   git push origin v0.1.0
   ```

4. 在 Actions 中等待 Linux、Windows、macOS 三个构建任务全部成功。任务会把安装包上传到同一个 Releases 草稿；草稿中的部分产物不表示整次构建成功。
5. 从草稿下载并验证对应系统的安装、启动和系统凭据存储，按实际结果更新 README 的平台状态与发布说明，再手动发布草稿。CI 构建成功仅能证明编译和打包通过，不能替代实机或交易流程验证。

工作流的准备阶段会核对标签与三个版本号，后续各平台构建同一个已确认的提交。已公开发布的版本不允许通过此工作流覆盖安装包，应使用新的版本号与标签。

## 重试与手动触发

工作流出现在默认分支后，可以从 **Actions → Build desktop installers → Run workflow** 触发，在 `tag` 中填写已存在的版本标签，例如 `v0.1.0`。构建始终使用该标签对应的代码，不是下拉框中任意分支的代码。

失败后可重跑失败任务，或再次手动构建同一标签；未发布的草稿会复用。所有产物通过验证前保持草稿状态，避免外发不完整的安装包。

工作流只使用自动提供的 `GITHUB_TOKEN`，通过 job 级别的 `contents: write` 权限创建草稿和上传附件。不需要配置个人访问令牌、Binance API Key 或付费签名密钥；若组织策略限制写权限，应检查该工作流的有效权限。
