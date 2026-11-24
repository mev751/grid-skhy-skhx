# Hyperliquid SKHY / SKHX 动态做空网格套利机器人 (Rust 版)

本项目由 Python 原版 (`HL_SKHY_STRAT_3.8.py`) 改造重构成高性能 Rust 异步架构，具备极低网络与签名延迟、零内存泄漏及高并发稳定性。

---

## 核心特性与机制

1. **2 次 Maker 挂单 + 兜底 Taker 扫单**：
   - **第 1 次 Maker**：买卖双侧（SKHX 买一，SKHY 卖一）并发提交 Alo（Post-Only / Add Liquidity Only）挂单，等待设定超时（默认 2 秒）。
   - **第 2 次 Maker**：若超时仍有未成交部分，撤单并刷新最新买一/卖一盘口重新提交 Alo 挂单，再等待 2 秒。
   - **兜底 Taker**：若第 2 次仍未完全成交，剩余数量立即降级为 IOC（Immediate Or Cancel）穿透盘口扫单，同时包含双腿失衡保护（某腿彻底失败时自动回滚强平单腿头寸），兼顾手续费节约与头寸风控。
2. **纯环境变量配置**：
   - 彻底摒弃 `.env` 文件，敏感私钥与配置仅从系统环境变量读取，安全可靠。
3. **原生 EIP-712 / Phantom Agent 签名**：
   - 基于纯 Rust `k256` + `sha3` + `rmp-serde` 实现 Hyperliquid 官方 L1 动作序列化与签名，支持 HIP-3 自定义 DEX（`xyz:` 命名空间）。
4. **全景对账控制台看板**：
   - 1:1 还原 Python v3.8 紧凑看板，实时呈现总净值、双腿真实仓位与账本、L2 盘口价差与阈值、最近 20 条对账与成交日志。
5. **GitHub Actions 自动化流水线**：
   - 自动编译 Linux AMD64 发布版本并自动通过 SSH/SCP 部署至目标服务器 `/home/emc/grid-skhy-skhx` 目录。

---

## 环境变量配置

程序通过读取系统环境变量进行参数配置，敏感私钥不在代码或文件中存储：

| 环境变量名 | 默认值 | 说明 |
| :--- | :--- | :--- |
| `HL_SECRET_KEY` | *(必填)* | 钱包私钥（64位十六进制字符，支持带或不带 `0x` 前缀） |
| `HL_WALLET_ADDR` | *(可选)* | 钱包地址（若留空则自动从私钥公钥计算推导） |
| `Q_USDC` | `1000` | 单次网格目标仓位额度 (USDC) |
| `SPLIT_N` | `1` | 单次网格订单切分次数 |
| `MAX_GRIDS` | `10` | 最大网格层数上限 |
| `P1` | `1.4` | 做空网格基准启动触发比值 |
| `STEP_X` | `0.003` | 网格步长 |
| `SLICE_DELAY_MS` | `750` | 切片间隔时长 (毫秒) |
| `LEVERAGE` | `10` | 策略杠杆倍数 |
| `EMA_ALPHA` | `0.25` | K值平滑指数 (0.1 ~ 1.0) |
| `MAKER_TIMEOUT_MS` | `2000` | 每次 Maker 挂单等待超时 (毫秒) |
| `MAKER_MAX_RETRIES` | `2` | Maker 尝试次数 (默认 2 次后转 Taker) |
| `WS_SKHY` | `xyz:SKHY` | SKHY 行情与交易代币标识 |
| `WS_SKHX` | `xyz:SKHX` | SKHX 行情与交易代币标识 |

---

## 本地编译与运行

### 编译
```bash
cargo build --release
```

### 运行
```bash
# 导出私钥环境变量后运行
export HL_SECRET_KEY="your_private_key_here"
./target/release/grid-skhy-skhx
```

---

## GitHub Actions 自动部署配置

当代码推送到 `main` 分支时，GitHub Actions 会自动触发构建并部署。

请在 GitHub 仓库的 **Settings -> Secrets and variables -> Actions** 中配置以下 Secrets：

1. `BOX_HOST`：`box` 服务器的公网 IP 或域名。
2. `BOX_USER`：SSH 登录用户名（默认 `emc`）。
3. `BOX_SSH_KEY`：用于登录服务器的 SSH 私钥。
4. `BOX_PORT`：SSH 端口（默认 `22`）。
5. `BOX_SSH_PASSPHRASE`：*(可选)* 若 SSH 私钥有密码保护，则填入该密码。

Actions 工作流会将二进制可执行程序部署至：
`/home/emc/grid-skhy-skhx/grid-skhy-skhx`
并自动赋予可执行权限 (`chmod +x`)。
