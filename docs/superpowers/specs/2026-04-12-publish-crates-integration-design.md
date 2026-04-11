# 设计文档: 集成 publish-crates Action

## 1. 目标 (Objective)
通过 GitHub Actions 自动将 `mt-engine` 项目的 crate 发布到 crates.io。使用 `katyo/publish-crates` 插件来简化多 crate 工作区的发布流程，并确保依赖顺序正确。

## 2. 背景 (Background)
该项目是一个 Rust 工作区，包含两个 crate:
- `mt_engine` (路径: `mt-engine-sbe`, 版本: 0.1.1)
- `mt-engine-core` (路径: `mt-engine-core`, 版本: 0.1.1)

目前已有 `rust.yml` 处理 CI (fmt, clippy, test)，但缺乏自动发布机制。

## 3. 方案设计 (Proposed Design)

### 3.1 触发机制 (Triggers)
- **Git Tags**: 当推送格式为 `v*` (例如 `v0.1.2`) 的标签时触发发布工作流。

### 3.2 工作流逻辑 (Workflow Logic)
创建一个新的工作流文件 `.github/workflows/release.yml`。

#### Job: `publish`
- **Runner**: `ubuntu-latest`
- **Steps**:
  1. **Checkout**: 检出代码。
  2. **Install Rust**: 安装稳定版 Rust 工具链。
  3. **Publish to crates.io**:
     - 使用 `katyo/publish-crates@v2`。
     - **参数配置**:
       - `registry-token`: 从 GitHub Secrets 读取 `${{ secrets.CARGO_REGISTRY_TOKEN }}`。
       - `ignore-unpublished-changes`: 设为 `true` (可选，防止因未变动版本而报错)。
       - `check-repo`: 设为 `true` (默认，确保发布的是当前代码)。

### 3.3 依赖处理 (Dependency Management)
`katyo/publish-crates` 会自动分析工作区依赖，按照 `mt_engine` -> `mt-engine-core` 的顺序发布，确保 `mt-engine-core` 发布时其依赖的 `mt_engine` 已在 crates.io 可用。

## 4. 安全性 (Security)
- 需要用户在 GitHub 仓库中手动添加 `CARGO_REGISTRY_TOKEN`。
- 该 Token 权限应仅限于发布 crate。

## 5. 测试与验证 (Verification)
- **Dry Run**: 在初次运行或测试时，可临时将 `dry-run: true` 添加到配置中以验证流程。
- **验证**: 推送一个测试标签 (如 `v0.0.0-test`) 查看 GitHub Actions 运行情况。

## 6. 成功标准 (Success Criteria)
- 推送 `v*` 标签后，GitHub Action 成功启动。
- 所有 crate 按照正确顺序发布到 crates.io。
- 工作流输出明确显示已发布的版本。
