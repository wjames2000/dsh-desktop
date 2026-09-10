#!/usr/bin/env bash
# 在目标平台上执行：下载 Node、安装 dsh、组装内置 profile
# 用法：./scripts/prepare-bundle.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUNDLE="$ROOT/bundle"
# 内置 Node 版本：固定值，保证构建可复现、跨构建机一致。
# 必须与构建机运行本脚本的 node 版本一致——npm ci 按当前 node 的 ABI 选择 native
# prebuilds（node-pty 等），内置 node 与被依赖模块的 ABI 不匹配会导致 sidecar 启动失败。
# 下面会做 fail-fast 校验；如需换版本，用 NODE_VERSION 显式覆盖并确保构建机 node 同版本。
NODE_VERSION="${NODE_VERSION:-24.15.0}"
# dsh 包版本。默认值固化在 bundle/dsh-lock/ 的 package.json + package-lock.json 中；
# 若通过 DSH_VERSION 覆盖为其他版本，脚本会退化为 npm install 并更新 bundle/dsh 内的 lockfile
# （bundle/dsh-lock/ 是 tracked 的固化版本，升级时需同步更新并重新生成 lockfile）
DSH_VERSION="${DSH_VERSION:-0.1.5-rc.1}"
# npm 缓存默认放 bundle 下（~/.npm 可能是 root 所有或不可写），可用 NPM_CACHE 覆盖
NPM_CACHE="${NPM_CACHE:-$BUNDLE/.npm-cache}"
# 默认用 npmmirror HTTPS（用户 ~/.npmrc 里的 registry.npm.taobao.org 是废弃的 HTTP 地址，
# 重定向会挂起），可用 REGISTRY 覆盖为官方源等
REGISTRY="${REGISTRY:-https://registry.npmmirror.com}"

# 构建机 node 版本校验：必须与内置 node 一致，否则 native 模块 ABI 不匹配。
HOST_NODE="$(node --version 2>/dev/null | sed 's/^v//' || true)"
if [ -z "$HOST_NODE" ]; then
  echo "错误：构建机 PATH 中找不到 node。请安装 Node v${NODE_VERSION} 并确保它在 PATH 中。" >&2
  exit 1
fi
if [ "$HOST_NODE" != "$NODE_VERSION" ]; then
  echo "错误：构建机 node 版本（v${HOST_NODE}）与内置 node 版本（v${NODE_VERSION}）不一致。" >&2
  echo "      npm ci 会按当前 node 的 ABI 安装 native 模块，与内置 node 不匹配会导致 sidecar 启动失败。" >&2
  echo "      解决：切换到 v${NODE_VERSION}（如 nvm use ${NODE_VERSION}），或用 NODE_VERSION=v${HOST_NODE} 覆盖内置版本。" >&2
  exit 1
fi

# 平台检测（未知平台直接报错退出，避免生成错误的下载 URL）
case "$(uname -s)" in
  Darwin)
    OS="darwin"
    case "$(uname -m)" in
      arm64) ARCH="arm64" ;;
      x86_64) ARCH="x64" ;;
      *) echo "unsupported arch: $(uname -m)" >&2; exit 1 ;;
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
  *)
    echo "unsupported platform: $(uname -s)/$(uname -m)" >&2
    exit 1
    ;;
esac

echo ">> 平台: ${OS}-${ARCH}, Node ${NODE_VERSION}, dsh ${DSH_VERSION}"

# 1. 下载官方 Node 发行包，仅解出 node 二进制
mkdir -p "$BUNDLE/node"
if [ "$OS" = "win" ]; then
  # Windows 官方只发布 .zip/.7z/.msi/.exe，没有 tar.gz；直接下载单文件 node.exe
  NODE_URL="https://nodejs.org/dist/v${NODE_VERSION}/win-x64/node.exe"
  echo ">> 下载 $NODE_URL"
  curl -fsSL "$NODE_URL" -o "$BUNDLE/node/node.exe"
  BUNDLE_NODE="$BUNDLE/node/node.exe"
else
  NODE_TGZ="node-v${NODE_VERSION}-${OS}-${ARCH}.tar.gz"
  NODE_URL="https://nodejs.org/dist/v${NODE_VERSION}/${NODE_TGZ}"
  echo ">> 下载 $NODE_URL"
  if [ ! -f "/tmp/$NODE_TGZ" ]; then
    curl -fsSL "$NODE_URL" -o "/tmp/$NODE_TGZ"
  fi
  tar -xzf "/tmp/$NODE_TGZ" -C /tmp
  NODE_BIN="/tmp/node-v${NODE_VERSION}-${OS}-${ARCH}/bin/node"
  cp "$NODE_BIN" "$BUNDLE/node/node"
  chmod +x "$BUNDLE/node/node"
  BUNDLE_NODE="$BUNDLE/node/node"
fi

# 2. 安装 dsh 包及依赖（npm ci 用固化的 lockfile，快速且可复现；版本被覆盖时才退化为 npm install）
mkdir -p "$BUNDLE/dsh"
cp "$ROOT/bundle/dsh-lock/package.json" "$ROOT/bundle/dsh-lock/package-lock.json" "$BUNDLE/dsh/"
cd "$BUNDLE/dsh"
PINNED_DSH="$(node -p "require('./package.json').dependencies['@deepseek-ai/dsh']" 2>/dev/null || true)"
if [ "$PINNED_DSH" != "$DSH_VERSION" ]; then
  echo ">> DSH_VERSION($DSH_VERSION) 与 dsh-lock 固化版本($PINNED_DSH)不一致，改用 npm install"
  npm install "@deepseek-ai/dsh@${DSH_VERSION}" --no-audit --no-fund --cache "$NPM_CACHE" \
    --registry "$REGISTRY" \
    --fetch-timeout 120000 --fetch-retries 3 --fetch-retry-mintimeout 20000
else
  npm ci --no-audit --no-fund --cache "$NPM_CACHE" --registry "$REGISTRY" \
    --fetch-timeout 120000 --fetch-retries 3 --fetch-retry-mintimeout 20000
fi

# 让 bundle/dsh/lib 指向 dsh 包的 lib 目录（sidecar.rs 期望的布局：dsh/lib/bin.js；
# 注意 bin.js 在包内 lib/ 下，所以目标是 node_modules/@deepseek-ai/dsh/lib，不是包根）
# 注意：必须用真实目录复制而非 symlink——tauri 打包 resources 时（tauri-bundler 的
# WalkDir follow_links=false + fs_utils::copy_file 要求源 is_file）不跟随 symlink，
# 用 symlink 会导致打包产物 .app 里 dsh/lib 缺失、sidecar 启动失败。
# 包内 lib 仅几十 KB，复制成本可忽略。
rm -rf "$BUNDLE/dsh/lib"
cp -r node_modules/@deepseek-ai/dsh/lib "$BUNDLE/dsh/lib"
# bin.js 是 ESM（import 语法）。symlink 时它归属于 dsh 包的 package.json（type: module）；
# 复制成真实目录后最近的 package.json 是 bundle/dsh/package.json（无 type: module），
# 会以 CJS 加载而失败。这里补一个 type: module 的 package.json 保持 ESM 语义。
echo '{"type": "module"}' > "$BUNDLE/dsh/lib/package.json"
# dsh >= 0.1.2：agent-presets 从 dsh 包的 config/ 移入独立的 @deepseek-ai/dsh-agent-presets
# 包（presets/ 目录，包内 import.meta.url 相对解析，随 node_modules 安装即可用）。
# 旧版（0.1.1）复制 config/ 的逻辑已不再需要；这里清理历史残留目录。
rm -rf "$BUNDLE/dsh/config" 2>/dev/null || true

# 清理历史遗留的嵌套缓存（防止打进安装包）
rm -rf "$BUNDLE/dsh/bundle" 2>/dev/null || true

# 3. 组装内置 profile（含 node_modules：插件市场等 out-of-tree 依赖必须随 profile 分发）
mkdir -p "$BUNDLE/profile"
cp -r "$ROOT/bundle/profile-template/." "$BUNDLE/profile/"
if [ -f "$BUNDLE/profile/package.json" ]; then
  if (cd "$BUNDLE/profile" && npm install --no-audit --no-fund --cache "$NPM_CACHE" --registry "$REGISTRY" \
      --fetch-timeout 120000 --fetch-retries 3 --fetch-retry-mintimeout 20000); then
    echo ">> profile 依赖安装完成"
  else
    echo "WARN: profile 依赖安装失败（插件市场可能不可用）"
  fi
else
  echo "WARN: bundle/profile-template 缺少 package.json，跳过 profile 依赖安装"
fi

# 4. 校验
echo ">> 校验 node 版本"
"$BUNDLE_NODE" --version
echo ">> 校验 dsh 可启动（--help 不启动服务，路径与 sidecar.rs 一致）"
"$BUNDLE_NODE" "$BUNDLE/dsh/lib/bin.js" --help >/dev/null
echo ">> 校验 native prebuilds"
ls "$BUNDLE/dsh/node_modules/node-pty/prebuilds" >/dev/null 2>&1 || echo "WARN: node-pty prebuilds 缺失"
# koffi 2.x 用 koffi/prebuilds/；3.x 改为平台 optionalDependencies 包 @koromix/koffi-<platform>
ls "$BUNDLE/dsh/node_modules/koffi/prebuilds" >/dev/null 2>&1 \
  || find "$BUNDLE/dsh/node_modules/@koromix" -name "*.node" -print -quit 2>/dev/null | grep -q . \
  || echo "WARN: koffi native 二进制缺失"
ls "$BUNDLE/profile/node_modules" >/dev/null 2>&1 || echo "WARN: profile node_modules 缺失（插件市场不可用）"
echo ">> 校验 agent presets（dsh-agent-presets 包内 shipped presets，dsh >= 0.1.2 结构）"
ls "$BUNDLE/dsh/node_modules/@deepseek-ai/dsh-agent-presets/presets" >/dev/null 2>&1 \
  || echo "WARN: dsh-agent-presets/presets 缺失（新建会话将失败）"

echo ">> 完成。bundle 目录：$BUNDLE"
