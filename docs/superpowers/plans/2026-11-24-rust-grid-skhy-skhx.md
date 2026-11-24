# Rust Hyperliquid SKHY-SKHX Arbitrage Grid Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port the Python Hyperliquid SKHY/SKHX arbitrage grid trading bot (`HL_SKHY_STRAT_3.8.py`) to Rust, implementing a 2-stage Maker order mechanism (with Taker fallback), environment-variable-based private key loading, Git repository setup with push to GitHub, and a GitHub Actions workflow for Linux AMD64 build and deployment to the box server.

**Architecture:**
- Asynchronous Rust service (`tokio`) handling real-time WebSocket feeds for L2 orderbooks and REST client for Hyperliquid `/info` and `/exchange`.
- Pure-Rust EIP-712 / phantom agent signing (`k256`, `sha3`, `rmp-serde`) for zero-dependency cross-compilation.
- Concurrent dual-leg order execution state machine supporting 2-attempt Alo (Post-Only Maker) orders with configurable timeout, auto-fallback to IOC Taker orders, and leg-imbalance hedging.
- GitHub Actions CI/CD to build a Linux x86_64 binary and transfer it via SSH/SCP to `/home/emc/grid-skhy-skhx`.

**Tech Stack:** Rust 2021, `tokio`, `tokio-tungstenite`, `reqwest`, `serde`, `serde_json`, `rmp-serde`, `k256`, `sha3`, `hex`, GitHub Actions.

---

## Global Constraints
- Private keys must ONLY be loaded from environment variable `HL_SECRET_KEY` (no `.env` file).
- No C/OpenSSL build dependencies to guarantee smooth cross-compilation on Linux musl/gnu (`rustls-tls` instead of native-tls).
- Target directory on box server: `/home/emc/grid-skhy-skhx`.
- Remote Git repository: `git@github.com:mev751/grid-skhy-skhx.git`.

---

### Task 1: Project Scaffolding & Configuration Module

**Files:**
- Create: `Cargo.toml`
- Create: `src/config.rs`
- Create: `src/types.rs`
- Test: `tests/test_config.rs`

- [ ] **Step 1: Write tests for configuration loading and validation**
- [ ] **Step 2: Create `Cargo.toml` with required dependencies (rustls, tokio, serde, k256, sha3, rmp-serde)**
- [ ] **Step 3: Implement `src/config.rs` reading `HL_SECRET_KEY`, `HL_WALLET_ADDR`, and grid parameters with defaults**
- [ ] **Step 4: Verify tests pass**

---

### Task 2: Pure-Rust Hyperliquid EIP-712 Signer & Action Serializer

**Files:**
- Create: `src/hyperliquid/signer.rs`
- Test: `tests/test_signer.rs`

- [ ] **Step 1: Write test vectors for EIP-712 exchange domain, Agent connectionId hash, and signature verification**
- [ ] **Step 2: Implement msgpack serialization of order and cancel actions (`rmp-serde`)**
- [ ] **Step 3: Implement keccak256 action hash and EIP-712 typed signing using `k256`**
- [ ] **Step 4: Run unit tests and verify signature outputs**

---

### Task 3: Hyperliquid REST & WebSocket Client

**Files:**
- Create: `src/hyperliquid/info.rs`
- Create: `src/hyperliquid/exchange.rs`
- Create: `src/hyperliquid/ws.rs`
- Create: `src/hyperliquid/mod.rs`

- [ ] **Step 1: Implement `/info` client for `perpDexs` (to calculate DEX asset index offset for `xyz`), `meta`, `clearinghouseState`, `userFills`, and `userFunding`**
- [ ] **Step 2: Implement `/exchange` client to send signed `order` and `cancel` requests**
- [ ] **Step 3: Implement WebSocket client for `wss://api.hyperliquid.xyz/ws` subscribing to `l2Book` for `xyz:SKHY` and `xyz:SKHX` with auto-reconnect and ping/pong**
- [ ] **Step 4: Write test verifying info parsing and payload construction**

---

### Task 4: 2-Stage Maker Order Execution Engine with Taker Fallback

**Files:**
- Create: `src/strategy/order_executor.rs`
- Create: `src/strategy/engine.rs`
- Create: `src/strategy/mod.rs`
- Test: `tests/test_engine.rs`

- [ ] **Step 1: Write tests for grid trigger thresholds and EMA smoothing calculation**
- [ ] **Step 2: Implement dual-leg concurrent Maker execution (Alo / Post-Only) with timeout check**
- [ ] **Step 3: Implement 2nd Maker attempt on timeout/partial fill, followed by Taker (IOC) fallback and leg-imbalance hedging**
- [ ] **Step 4: Implement grid position state management, startup position alignment, and excess position clearing at layer 0**
- [ ] **Step 5: Run tests to verify state transitions and fallback logic**

---

### Task 5: Terminal Dashboard UI & Application Main Entry

**Files:**
- Create: `src/ui.rs`
- Create: `src/main.rs`

- [ ] **Step 1: Implement console UI rendering replicating Python v3.8 layout (account equity, positions, L2 prices, trigger lines, rolling logs)**
- [ ] **Step 2: Implement `src/main.rs` wiring up config, background equity tracker, WebSocket listener, strategy engine, and UI loop**
- [ ] **Step 3: Test compilation and verify binary runs in dry-run/simulation mode without credentials**

---

### Task 6: GitHub Actions Workflow, Documentation & Git Push

**Files:**
- Create: `.github/workflows/deploy.yml`
- Create: `.gitignore`
- Create: `README.md`

- [ ] **Step 1: Initialize git repository, configure `.gitignore` (ignoring target/, sensitive files)**
- [ ] **Step 2: Create `.github/workflows/deploy.yml` with Linux x86_64 build and SSH/SCP deployment to `/home/emc/grid-skhy-skhx`**
- [ ] **Step 3: Write comprehensive `README.md` with environment variables, build instructions, and GitHub Secrets setup**
- [ ] **Step 4: Commit changes and push to `git@github.com:mev751/grid-skhy-skhx.git`**
