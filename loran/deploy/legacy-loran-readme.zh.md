# Loran 部署工具（loran-deploy）

`loran-deploy` 是 loran 二进制安装与 systemd 部署的统一工具：从 GitHub Release 下载或
使用本地二进制，校验、安装、升级和回滚 loran，并部署 systemd 服务。不依赖目标机器上的源码或
Rust toolchain。

## 安装模型

版本化二进制放在 `releases/<version>/loran`，活跃版本通过 `bin/loran` 软链接指向：

```text
/opt/loran/releases/vX.Y.Z/loran
/opt/loran/releases/v0.4.4/loran
/opt/loran/bin/loran -> /opt/loran/releases/v0.4.4/loran
/opt/loran/shared/   # 运行时共享数据
/opt/loran/logs/     # 日志
/opt/loran/db/       # SQLite 数据
```

systemd 的 `ExecStart` 固定指向 `bin/loran`，所以切换版本只需重指软链接 + 重启服务，
**永不修改 systemd unit**。配置、数据库、日志、证书放在版本目录之外，切换版本不影响状态。
`update` 和 `rollback` 必须显式传 `--restart-service`，工具不会只切软链接而留下旧进程继续运行。
重启后工具会核对 systemd `MainPID` 的真实可执行文件是否等于目标 release binary，不一致则失败并回滚。

版本目录是不可变的：同一个 `<version>` 如果已存在，只有二进制 checksum 完全一致时才复用；
如果同版本内容不同，工具会拒绝覆盖。需要发布新内容时请使用新的 tag/version，保证回滚目标仍然可靠。

## 安装 loran-deploy

从 GitHub Release 下载（首个版本可人工下载）：

```bash
curl -LO https://github.com/Actrium/actrium/releases/download/vX.Y.Z/loran-deploy-linux-x86_64
curl -LO https://github.com/Actrium/actrium/releases/download/vX.Y.Z/loran-deploy-linux-x86_64.sha256
sha256sum -c loran-deploy-linux-x86_64.sha256
sudo install -m 0755 loran-deploy-linux-x86_64 /usr/local/bin/deploy
```

## 新部署一套 loran 服务（完整流程）

在一台全新机器上从零部署单实例。多实例见末尾。

### 前置条件

- systemd Linux（Ubuntu 24.04 已验证），x86_64 或 arm64，root 权限
- 开放端口：HTTP（如 8080）、HTTPS（如 443）、ICE/TURN（3478）、relay 范围（49152-65535）
- 域名 DNS 指向本机 + TLS 证书
- 准备好：`loran-deploy` 工具、`loran` 二进制、`config.toml`、证书

### 1. 安装 loran-deploy 工具

见上方「安装 loran-deploy」；或从构建机 `scp` 任意 `deploy` 二进制到
`/usr/local/bin/deploy`。

### 2. 准备配置与证书

建议都放在安装目录 `/opt/loran` 下，这样 `WorkingDirectory` 默认即此目录，
config 中的相对路径自动正确：

```text
/opt/loran/
├── config.toml
├── certificates/
│   ├── your.domain.crt
│   └── your.domain.key
└── (database/ signer.db 运行时自动生成)
```

`config.toml` 中 `cert`/`key`/`sqlite_path`/`signer.db` 可用相对路径（相对
`/opt/loran`）。各 secret（`loran_shared_key`、`renewal_token_secret`、
`shared_secret`、admin `password`）请用本机随机值。

### 3. 安装 loran 二进制

```bash
# 方式 A：从 GitHub Release（优先用 .sha256；没有时使用 GitHub asset digest）
sudo deploy install --tag vX.Y.Z --install-dir /opt/loran --no-path

# 方式 B：本地二进制（离线/灰度发布）
sudo deploy install --binary-path /root/loran \
  --version vX.Y.Z --sha256-path /root/loran.sha256 \
  --install-dir /opt/loran --no-path
# 本地调试才可加 --skip-verify；Release 模式始终强制 SHA-256 校验
```

生成 `/opt/loran/releases/vX.Y.Z/loran` 与 `/opt/loran/bin/loran` 软链。

### 4. 创建 systemd 服务并启动

```bash
sudo deploy service \
  --service-name loran --install-dir /opt/loran \
  --config /opt/loran/config.toml \
  --user loran --group loran \
  --working-directory /opt/loran \
  --force-overwrite-unit
```

工具会：检测 systemd → 创建/确认用户组 → 校验二进制和配置 → 授权运行时目录 →
生成加固 unit（`bin/loran` 入口；端口 <1024 自动加 `CAP_NET_BIND_SERVICE`；
`ProtectSystem=strict`；`ReadWritePaths` 按 config 自动算；`bin/`、`releases/` 显式只读）→ daemon-reload →
enable → start；并询问是否应用 ufw 防火墙规则。

> **非 root 用户**：`service` 会把 `logs/`、`db/`、`shared/` 授权给服务用户，
> 但 `bin/`、`releases/` 保持 root 管理。不要 `chown -R /opt/loran`，否则服务
> 进程可以改自己的二进制。证书、配置文件请单独保证服务用户可读。
>
> **`--working-directory`**：当 config 的相对路径相对的是安装目录之外的目录时
> （如历史部署的 `/opt/actr-project/loran`），用它指定真实工作目录；相对路径与
> `ReadWritePaths` 都按它解析。全新部署把文件都放 `/opt/loran` 下则无需此参数。

### 5. 验证

```bash
deploy status --install-dir /opt/loran --service-name loran  # 当前版本 + 运行版本 + 已装版本
systemctl is-active loran                          # active
journalctl -u loran -f                             # 日志
curl -k https://localhost/issuer/health                # 健康检查（端口见 config）
```

### 升级 / 回滚

```bash
sudo deploy update   --tag v0.4.4 --install-dir /opt/loran --restart-service loran
sudo deploy rollback --to vX.Y.Z  --install-dir /opt/loran --restart-service loran
```

升级失败自动回滚到上一版本。生产建议加 `--health-url`，避免服务进程 active 但接口不可用：

```bash
sudo deploy update --tag v0.4.4 \
  --install-dir /opt/loran \
  --restart-service loran \
  --health-url http://127.0.0.1:8080/health
```

### 多实例（一台机器跑多套）

用不同 `--service-name` + 不同 `--install-dir` + 不同 config（端口不可冲突）：

```bash
sudo deploy install --tag vX.Y.Z --install-dir /opt/loran-a --no-path
sudo deploy service --service-name loran-a --install-dir /opt/loran-a \
  --config /opt/loran-a/config.toml --user loran --group loran --working-directory /opt/loran-a

sudo deploy install --tag vX.Y.Z --install-dir /opt/loran-b --no-path
sudo deploy service --service-name loran-b --install-dir /opt/loran-b \
  --config /opt/loran-b/config.toml --user loran --group loran --working-directory /opt/loran-b
```

两个 unit 名不同，各自独立升级/回滚，互不影响。

## 命令

```bash
# 依赖检查
deploy deps

# 从 GitHub Release 首次安装
sudo deploy install --tag vX.Y.Z --install-dir /opt/loran --no-path

# 从本地二进制安装（离线/灰度，需 --sha256-path 或 --skip-verify）
sudo deploy install --binary-path ./loran-linux-x86_64 \
  --sha256-path ./loran-linux-x86_64.sha256 --version vX.Y.Z \
  --install-dir /opt/loran --no-path

# 开发：用本地 target/release/loran 构建
sudo deploy install --from-local-build --install-dir /opt/loran

# 部署/重建 systemd 服务（已存在默认拒绝覆盖，--force-overwrite-unit 才覆盖）
sudo deploy service --service-name loran2 --install-dir /opt/loran \
  --config /etc/loran/config.toml --user actor-rtc --group actor-rtc

# 升级（切软链接 + 重启服务；失败自动回滚到上一版本）
sudo deploy update --tag v0.4.4 --install-dir /opt/loran \
  --restart-service loran2 --health-url http://127.0.0.1:8080/health

# 回滚到已安装的版本
sudo deploy rollback --to vX.Y.Z --install-dir /opt/loran \
  --restart-service loran2 --health-url http://127.0.0.1:8080/health

# 查看当前版本、运行版本与已安装版本
deploy status --install-dir /opt/loran --service-name loran2

# 卸载（分组确认；默认保留 db/logs/shared 和配置）
sudo deploy uninstall --install-dir /opt/loran --service-name loran2
```

## 二进制来源与校验

`install`/`update` 二进制来源三选一：`--tag`、`--latest`、`--binary-path`。

- Release 模式（`--tag`/`--latest`）：强制 SHA-256 校验；优先使用 `.sha256` sidecar，缺失时使用 GitHub Release API 返回的 `sha256:` asset digest，二者都没有或不一致即失败。
- 本地模式（`--binary-path`）：默认要求 `--sha256-path`；`--skip-verify` 可跳过（打印强警告，不用于生产）。
- 开发模式（`--from-local-build`）：仅 `install` 支持，使用本机 `target/release/loran`，自动使用 `local` version 并跳过校验。
- `--version` 只用于本地二进制/本地构建；Release 模式使用 GitHub Release 的 tag。
- version 只能包含字母、数字、`.`、`_`、`-`、`+`，不能包含 `/`、`\`、空白或 `..`。
- 同一 version 内容不可变：已安装的 version 如果 checksum 不同，`install`/`update` 都会拒绝覆盖。
- `update`/`rollback` 必须指定 `--restart-service`，确保活跃软链、运行进程和健康检查处在同一个发布动作里；重启后会校验实际运行的 `/proc/$MAINPID/exe`。

TODO: GitHub Release downloads currently verify SHA-256 integrity only. They do not verify publisher authenticity. Before enabling unattended auto-upgrade, add deploy-side publisher signature verification with Sigstore/cosign or GPG.

## 环境变量

| 变量 | 作用 |
|------|------|
| `GITHUB_TOKEN` | 私有仓库下载用，仅需 Contents Read 权限。 |
| `LORAN_REPOSITORY` | GitHub owner/repo，默认 `Actrium/actrium`。 |
| `LORAN_HEALTH_URL` | `update`/`rollback` 重启后的 HTTP readiness 探测地址，可用 `--health-url` 覆盖。 |
| `LORAN_HEALTH_WAIT_SECONDS` | `update`/`rollback` 重启后等待 ready 的秒数，默认 5，最大 300。 |
| `LORAN_ALLOW_LEGACY_INSTALL_SH` | 设为 `1` 才允许执行 legacy `install.sh`。默认拒绝运行。 |

## 约束

- `service` 仅支持 systemd，先检测再执行。
- 安装目录必须是 `/opt` 下的绝对路径子目录，例如 `/opt/loran`、`/opt/loran-a`。
- 安装目录会拒绝 `/`、`/opt`、`/home`、`/tmp`、`..` 和已有 symlink 组件。
- loran 托管二进制名固定为 `loran`，不支持自定义 binary name。
- `update` 永不写 systemd unit；unit 加固由 `service`/人工运维维护。
- 服务名建议显式传入（`--service-name`），多实例时必须区分 unit 名。
- 服务用户/组名会按 Linux 账号名规则校验；默认 `loran`。

## Legacy install.sh

`install.sh` 仅保留用于紧急/历史恢复，不再作为正式部署入口。它没有 Release checksum、
版本目录、软链接切换和不可变版本保护，因此默认直接退出。确需执行时必须显式设置：

```bash
LORAN_ALLOW_LEGACY_INSTALL_SH=1 sudo -E ./install.sh install
```

新部署和升级都应使用 `deploy install/service/update/rollback`。

## 可选配置文件

`/etc/loran/deploy.toml`（计划中，当前未自动加载；CLI 参数与环境变量为准）示例见
`deploy.toml.example`。
