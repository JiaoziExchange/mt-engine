#!/bin/bash
set -e

# 颜色定义
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${BLUE}=== Starting MT-Engine Snapshot E2E Portability Tests ===${NC}\n"

# 切换到 core 目录执行测试
cd mt-engine-core

# 1. 测试生产者/标准节点 (Sparse + Snapshot Export)
echo -e "${BLUE}[Step 1] Testing Producer Node (SparseBackend + Export Support)...${NC}"
if cargo test test_e2e_snapshot_portability --features snapshot -- --nocapture; then
    echo -e "${GREEN}✔ Producer Node Test Passed${NC}\n"
else
    echo -e "${RED}✘ Producer Node Test Failed${NC}"
    exit 1
fi

# 2. 测试计算节点 (Dense + Recovery Only)
echo -e "${BLUE}[Step 2] Testing Consumer Node (DenseBackend + Recovery Only)...${NC}"
if cargo test test_e2e_snapshot_portability --features dense-node,serde -- --nocapture; then
    echo -e "${GREEN}✔ Consumer Node (Dense) Test Passed${NC}\n"
else
    echo -e "${RED}✘ Consumer Node (Dense) Test Failed${NC}"
    exit 1
fi

# 3. 验证极致纯净模式 (No Snapshot support at all)
echo -e "${BLUE}[Step 3] Verifying Pure Performance Compilation (No Snapshot deps)...${NC}"
if cargo check --features dense-node; then
    echo -e "${GREEN}✔ Pure Performance Build Verified${NC}\n"
else
    echo -e "${RED}✘ Pure Performance Build Failed${NC}"
    exit 1
fi

echo -e "${GREEN}所有快照移植性测试全部通过！${NC}"
