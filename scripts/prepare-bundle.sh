#!/usr/bin/env bash
# 在目标平台上执行：下载 Node、安装 dsh、组装内置 profile
# 用法：./scripts/prepare-bundle.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUNDLE="$ROOT/bundle"
NODE_VERSION="${NODE_VERSION:-$(node --version | sed 's/^v//')}"  # 默认与构建机一致
# npm 缓存默认放 bundle 下（~/.npm 可能是 root 所有或不可写），可用 NPM_CACHE 覆盖
NPM_CACHE="${NPM_CACHE:-$BUNDLE/.npm-cache}"
# 默认用 npmmirror HTTPS（用户 ~/.npmrc 里的 registry.npm.taobao.org 是废弃的 HTTP 地址，
# 重定向会挂起），可用 REGISTRY 覆盖为官方源等
REGISTRY="${REGISTRY:-https://registry.npmmirror.com}"

# 平台检测
case "$(uname -s)" in
  Darwin)
    OS="darwin"
    case "$(uname -m)" in
      arm64) ARCH="arm64" ;;
      x86_64) ARCH="x64" ;;
    esac
    ;;
  MINGW*|MSYS*|CYGWIN*)
    OS="win"
    ARCH="x64"
    ;;
  Linux)
    OS="linux"
    case "$(uname -m)" in
      aarch64) ARCH="arm64" ;;
      *) ARCH="x64" ;;
    esac
    ;;
esac

echo ">> 平台: ${OS}-${ARCH}, Node ${NODE_VERSION}"

# 1. 下载官方 Node 发行包，仅解出 node 二进制
mkdir -p "$BUNDLE/node"
NODE_TGZ="node-v${NODE_VERSION}-${OS}-${ARCH}.tar.gz"
NODE_URL="https://nodejs.org/dist/v${NODE_VERSION}/${NODE_TGZ}"
echo ">> 下载 $NODE_URL"
if [ ! -f "/tmp/$NODE_TGZ" ]; then
  curl -fsSL "$NODE_URL" -o "/tmp/$NODE_TGZ"
fi
tar -xzf "/tmp/$NODE_TGZ" -C /tmp
if [ "$OS" = "win" ]; then
  NODE_BIN="/tmp/node-v${NODE_VERSION}-${OS}-${ARCH}/node.exe"
  cp "$NODE_BIN" "$BUNDLE/node/node.exe"
else
  NODE_BIN="/tmp/node-v${NODE_VERSION}-${OS}-${ARCH}/bin/node"
  cp "$NODE_BIN" "$BUNDLE/node/node"
  chmod +x "$BUNDLE/node/node"
fi

# 2. 安装 dsh 包及依赖（npm ci 语义：清空后重装，保证 prebuilds 与平台匹配）
mkdir -p "$BUNDLE/dsh"
cd "$BUNDLE/dsh"
if [ ! -f package.json ]; then
  npm init -y >/dev/null 2>&1
fi
npm install "@deepseek-ai/dsh@0.1.1-rc.2" --no-audit --no-fund --cache "$NPM_CACHE" \
  --registry "$REGISTRY" \
  --fetch-timeout 120000 --fetch-retries 3 --fetch-retry-mintimeout 20000

# 3. 组装内置 profile
mkdir -p "$BUNDLE/profile"
cp -r "$ROOT/bundle/profile-template/." "$BUNDLE/profile/"

# 4. 校验
echo ">> 校验 node 版本"
"$BUNDLE/node/node" --version
echo ">> 校验 dsh 可启动（--help 不启动服务）"
"$BUNDLE/node/node" "$BUNDLE/dsh/node_modules/@deepseek-ai/dsh/lib/bin.js" --help >/dev/null
echo ">> 校验 native prebuilds"
ls "$BUNDLE/dsh/node_modules/node-pty/prebuilds" >/dev/null 2>&1 || echo "WARN: node-pty prebuilds 缺失"
# koffi 2.x 用 koffi/prebuilds/；3.x 改为平台 optionalDependencies 包 @koromix/koffi-<platform>
ls "$BUNDLE/dsh/node_modules/koffi/prebuilds" >/dev/null 2>&1 \
  || find "$BUNDLE/dsh/node_modules/@koromix" -name "*.node" -print -quit 2>/dev/null | grep -q . \
  || echo "WARN: koffi native 二进制缺失"

echo ">> 完成。bundle 目录：$BUNDLE"
