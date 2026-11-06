#!/usr/bin/env node
// dytes 发布工具：自动升版、版本校验、打 tag 并推送。
//
// 常用命令：
//   node scripts/release.mjs                 # 自动 patch +1，提交、打 tag、推送
//   node scripts/release.mjs --current       # 使用当前清单版本（已手工改好版本时）
//   node scripts/release.mjs --check         # 只校验：三处版本一致、tag 对应、且大于线上
//   node scripts/release.mjs --bump minor    # 升次版本
//   node scripts/release.mjs --set-version 0.1.12
//   node scripts/release.mjs --no-push       # 只提交并打 tag，不推送
//
// 发布前校验规则：
//   - dytes.json / Cargo.toml / Cargo.lock 三处版本必须一致；
//   - 线上 https://packages.funxdata.com/core/dytes/dytes.json 的版本必须小于新版本；
//   - tag（如 v0.1.12）必须与清单版本一致，且不能已存在。

import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const MANIFEST_PATH = path.join(ROOT, "dytes.json");
const CARGO_TOML_PATH = path.join(ROOT, "Cargo.toml");
const CARGO_LOCK_PATH = path.join(ROOT, "Cargo.lock");
const DEFAULT_INDEX =
  "https://packages.funxdata.com/core/dytes/dytes.json";

function fail(message) {
  console.error(`\n错误: ${message}\n`);
  process.exit(1);
}

function info(message) {
  console.log(message);
}

function parseArgs(argv) {
  const options = {
    check: false,
    current: false,
    bump: "patch",
    setVersion: null,
    tag: null,
    push: true,
    commit: true,
    allowSame: false,
    offline: false,
    branch: "main",
    index: process.env.DYTES_RELEASE_INDEX || DEFAULT_INDEX,
  };

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    switch (arg) {
      case "--check":
        options.check = true;
        break;
      case "--current":
        options.current = true;
        break;
      case "--bump":
        options.bump = argv[++i];
        break;
      case "--set-version":
        options.setVersion = argv[++i];
        break;
      case "--tag":
        options.tag = argv[++i];
        break;
      case "--no-push":
        options.push = false;
        break;
      case "--no-commit":
        options.commit = false;
        break;
      case "--allow-same":
        options.allowSame = true;
        break;
      case "--offline":
        options.offline = true;
        break;
      case "--branch":
        options.branch = argv[++i];
        break;
      case "--index":
        options.index = argv[++i];
        break;
      case "--help":
      case "-h":
        console.log(
          [
            "用法: node scripts/release.mjs [选项]",
            "",
            "  --check                只校验，不修改文件",
            "  --current              使用当前版本，不自动升版",
            "  --bump <patch|minor|major>  自动升版类型（默认 patch）",
            "  --set-version <x.y.z>  指定版本",
            "  --tag <vX.Y.Z>         校验 tag 与版本一致",
            "  --no-push              不推送远程",
            "  --no-commit            只改版本文件，不提交",
            "  --allow-same           允许与线上版本相同（重发）",
            "  --offline              跳过线上版本检查",
            "  --branch <name>        发布分支（默认 main）",
            "  --index <url>          覆盖线上清单地址",
          ].join("\n"),
        );
        process.exit(0);
        break;
      default:
        fail(`未知参数: ${arg}`);
    }
  }

  if (!["patch", "minor", "major"].includes(options.bump)) {
    fail(`--bump 只能是 patch / minor / major，当前为 ${options.bump}`);
  }

  return options;
}

function readText(file) {
  return readFileSync(file, "utf8");
}

function writeText(file, content) {
  writeFileSync(file, content, "utf8");
}

function readVersions() {
  let manifest;
  try {
    manifest = JSON.parse(readText(MANIFEST_PATH));
  } catch (error) {
    fail(`读取 dytes.json 失败: ${error.message}`);
  }

  const cargoMatch = readText(CARGO_TOML_PATH).match(
    /\[package\][\s\S]*?^version = "([^"]+)"/m,
  );
  if (!cargoMatch) {
    fail("Cargo.toml 中未找到 [package] version");
  }

  const lockText = readText(CARGO_LOCK_PATH);
  const lockMatch = lockText.match(
    /\[\[package\]\]\r?\nname = "dytes"\r?\nversion = "([^"]+)"/,
  );
  if (!lockMatch) {
    fail('Cargo.lock 中未找到 name = "dytes" 的包');
  }

  return {
    manifest: manifest.version,
    cargo: cargoMatch[1],
    lock: lockMatch[1],
  };
}

function assertVersionsInSync(versions) {
  const { manifest, cargo, lock } = versions;
  if (manifest !== cargo || manifest !== lock) {
    fail(
      `版本不一致: dytes.json=${manifest} Cargo.toml=${cargo} Cargo.lock=${lock}\n` +
        "先运行 `cargo update -p dytes --precise " +
        manifest +
        "` 或手工同步后再发布。",
    );
  }
  assertSemver(manifest);
  return manifest;
}

function assertSemver(version) {
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version)) {
    fail(`版本号不是合法 semver: ${version}`);
  }
}

function compareVersion(a, b) {
  const parse = (value) => {
    const match = /^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$/.exec(value);
    if (!match) {
      fail(`无法比较版本: ${value}`);
    }
    return {
      major: Number(match[1]),
      minor: Number(match[2]),
      patch: Number(match[3]),
      pre: match[4] || "",
    };
  };

  const left = parse(a);
  const right = parse(b);
  for (const key of ["major", "minor", "patch"]) {
    if (left[key] !== right[key]) {
      return left[key] > right[key] ? 1 : -1;
    }
  }

  if (left.pre === right.pre) return 0;
  if (!left.pre) return 1;
  if (!right.pre) return -1;
  return left.pre.localeCompare(right.pre, "en", { numeric: true });
}

function bumpVersion(current, kind) {
  const match = /^(\d+)\.(\d+)\.(\d+)$/.exec(current);
  if (!match) {
    fail(`自动升版只支持 x.y.z 形式，当前为 ${current}`);
  }
  const [major, minor, patch] = match.slice(1).map(Number);
  if (kind === "major") return `${major + 1}.0.0`;
  if (kind === "minor") return `${major}.${minor + 1}.0`;
  return `${major}.${minor}.${patch + 1}`;
}

function updateVersionFiles(oldVersion, newVersion) {
  const manifestText = readText(MANIFEST_PATH);
  writeText(
    MANIFEST_PATH,
    manifestText.replace(
      `"version": "${oldVersion}"`,
      `"version": "${newVersion}"`,
    ),
  );

  const cargoText = readText(CARGO_TOML_PATH);
  writeText(
    CARGO_TOML_PATH,
    cargoText.replace(
      /(\[package\][\s\S]*?^version = ")[^"]+(")/m,
      `$1${newVersion}$2`,
    ),
  );

  const lockText = readText(CARGO_LOCK_PATH);
  writeText(
    CARGO_LOCK_PATH,
    lockText.replace(
      /(\[\[package\]\]\r?\nname = "dytes"\r?\nversion = ")[^"]+(")/,
      `$1${newVersion}$2`,
    ),
  );
}

async function fetchOnlineVersion(indexUrl) {
  const url = `${indexUrl}${indexUrl.includes("?") ? "&" : "?"}t=${Date.now()}`;
  let lastError;

  for (let attempt = 1; attempt <= 3; attempt += 1) {
    try {
      const response = await fetch(url, {
        headers: { "cache-control": "no-cache", pragma: "no-cache" },
        signal: AbortSignal.timeout(15000),
      });
      if (response.status === 404) {
        return null;
      }
      if (!response.ok) {
        throw new Error(`HTTP ${response.status}`);
      }
      const manifest = await response.json();
      if (!manifest.version) {
        throw new Error("清单中没有 version 字段");
      }
      return manifest.version;
    } catch (error) {
      lastError = error;
      if (attempt < 3) {
        await new Promise((resolve) => setTimeout(resolve, 2000 * attempt));
      }
    }
  }

  throw new Error(`读取线上版本失败 (${indexUrl}): ${lastError.message}`);
}

function git(args, options = {}) {
  const output = execFileSync("git", args, {
    cwd: ROOT,
    encoding: "utf8",
    stdio: options.stdio || "pipe",
  });
  return options.stdio === "inherit" || output == null ? "" : output.trim();
}

function gitOk(args) {
  try {
    git(args);
    return true;
  } catch {
    return false;
  }
}

function assertCleanWorktree() {
  const status = git(["status", "--porcelain", "--untracked-files=no"]);
  if (status) {
    fail(
      "工作区有未提交的改动，发布前请先提交：\n" +
        status
          .split("\n")
          .map((line) => `  ${line}`)
          .join("\n"),
    );
  }
}

function assertBranch(expected) {
  const branch = git(["rev-parse", "--abbrev-ref", "HEAD"]);
  if (branch !== expected) {
    fail(`当前分支是 ${branch}，请切换到 ${expected} 后再发布。`);
  }
}

function assertTagAvailable(tag) {
  if (gitOk(["rev-parse", "-q", "--verify", `refs/tags/${tag}`])) {
    fail(`tag ${tag} 已存在，每个版本只能发布一次。`);
  }
}

function assertRemoteTagAvailable(tag) {
  try {
    const output = git(["ls-remote", "--tags", "origin", `refs/tags/${tag}`]);
    if (output) {
      fail(`远程已存在 tag ${tag}，不能重复发布。`);
    }
  } catch {
    info("提示: 无法访问远程仓库，跳过远程 tag 检查。");
  }
}

function commitAndTag(version, tag, options) {
  git(["add", "dytes.json", "Cargo.toml", "Cargo.lock"]);

  const hasStagedChanges = !gitOk(["diff", "--cached", "--quiet"]);
  if (hasStagedChanges) {
    git(["commit", "-m", `chore(release): v${version}`], { stdio: "inherit" });
    info(`已提交版本 ${version}`);
  } else {
    info(`版本文件无变化，直接使用当前提交打 tag ${tag}`);
  }

  git(["tag", tag]);
  info(`已创建 tag ${tag}`);
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const current = assertVersionsInSync(readVersions());

  if (options.tag) {
    const expectedTag = `v${current}`;
    if (options.tag !== expectedTag) {
      fail(`tag ${options.tag} 与清单版本不一致，应为 ${expectedTag}`);
    }
  }

  const next = (() => {
    if (options.check) return current;
    if (options.setVersion) {
      assertSemver(options.setVersion);
      if (compareVersion(options.setVersion, current) <= 0) {
        fail(
          `--set-version ${options.setVersion} 必须大于当前版本 ${current}`,
        );
      }
      return options.setVersion;
    }
    if (options.current) return current;
    return bumpVersion(current, options.bump);
  })();

  info(`当前版本: ${current}`);
  if (next !== current) {
    info(`目标版本: ${next}`);
  }

  if (options.check) {
    info("版本文件一致 ✔");
  }

  const runChecks = async () => {
    if (options.offline) {
      info("已跳过线上版本检查 (--offline)");
      return;
    }

    let online;
    try {
      online = await fetchOnlineVersion(options.index);
    } catch (error) {
      // CDN / 网络故障不应阻断发布：版本文件与 tag 仍会严格校验。
      // 需要强制比较时可使用 --index 或修复网络后重跑。
      info(`警告: ${error.message}；跳过线上版本比较，继续发布`);
      return;
    }

    if (!online) {
      info("线上尚无 dytes 清单，按首次发布处理");
      return;
    }

    info(`线上版本: ${online}`);
    const comparison = compareVersion(next, online);
    if (comparison < 0) {
      fail(`目标版本 ${next} 小于线上版本 ${online}，拒绝回退发布。`);
    }
    if (comparison === 0 && !options.allowSame) {
      fail(
        `目标版本 ${next} 与线上版本相同。每次发布必须升一个小版本；` +
          "确需重发请加 --allow-same。",
      );
    }
    info(`版本校验通过: ${next} > ${online}`);
  };

  return (async () => {
    await runChecks();

    if (options.check) {
      info("校验通过 ✔");
      return;
    }

    const syncVersionFiles = () => {
      if (next === current) return;
      updateVersionFiles(current, next);
      const updated = assertVersionsInSync(readVersions());
      if (updated !== next) {
        fail(`版本文件更新失败，读到 ${updated}，期望 ${next}`);
      }
      info("已同步 dytes.json / Cargo.toml / Cargo.lock");
    };

    if (!options.commit) {
      syncVersionFiles();
      info("已跳过提交与打 tag (--no-commit)");
      return;
    }

    // 先确认工作区干净，再改版本文件；否则会把发布前的改动漏在 tag 之外。
    assertCleanWorktree();
    assertBranch(options.branch);
    const tag = `v${next}`;
    assertTagAvailable(tag);
    if (options.push) {
      assertRemoteTagAvailable(tag);
    }

    syncVersionFiles();
    commitAndTag(next, tag, options);

    if (options.push) {
      info(`推送 ${options.branch} 与 ${tag} ...`);
      git(["push", "origin", `HEAD:${options.branch}`], { stdio: "inherit" });
      git(["push", "origin", tag], { stdio: "inherit" });
      info(`发布完成: ${tag}`);
    } else {
      info(`已打 tag ${tag}，未推送 (--no-push)`);
    }
  })().catch((error) => {
    fail(error.message);
  });
}

main();
