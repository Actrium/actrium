# 运维脚本

本目录包含 actrix 项目的运维维护脚本。

## 脚本列表

### backup.sh - 数据备份

备份配置、数据库和日志文件。

**功能**：
- 备份 config.toml 配置文件
- 备份所有 SQLite 数据库（使用 VACUUM）
- 备份最近 7 天的日志
- 压缩为 tar.gz 格式
- 自动清理 30 天前的旧备份

**使用方法**：
```bash
# 默认备份到 ./backup/ 目录
bash scripts/backup.sh

# 自定义备份目录
BACKUP_DIR=/path/to/backup bash scripts/backup.sh
```

**恢复数据**：
```bash
tar -xzf backup/20251109_153000.tar.gz
```

---

### security-check.sh - 安全检查

运行安全检查，确保配置符合安全要求。

**检查项**：
- 数据库文件权限（应为 600）
- 默认密钥检测（禁止使用默认值）
- 密钥长度检查（≥16 字符）
- TLS 证书有效期检查
- 依赖漏洞扫描（cargo audit）

**使用方法**：
```bash
bash scripts/security-check.sh
```

**输出示例**：
```
🔍 Actrix 安全检查...

1. 检查数据库文件权限...
  ✅ database.db

2. 检查默认密钥...
  ✅ 未发现默认密钥

3. 检查 actrix_shared_key 长度...
  ✅ 密钥长度 32

4. 检查 TLS 证书...
  ✅ 证书存在: certificates/server.crt
     过期时间: Dec 31 23:59:59 2025 GMT

5. 检查依赖漏洞...
  ✅ 无已知漏洞

✅ 安全检查完成
```

---

## 常用命令快速参考

### 开发
```bash
# 构建 release 版本
cargo build --release

# 运行所有测试
cargo test --all

# 生成代码覆盖率报告
cargo install cargo-tarpaulin  # 首次需要安装
cargo tarpaulin --out Html --output-dir ./coverage
```

### 安全与维护
```bash
# 安全检查
bash scripts/security-check.sh

# 数据备份
bash scripts/backup.sh

# 依赖漏洞审计
cargo install cargo-audit      # 首次需要安装
cargo audit
```

### Docker
```bash
# 构建镜像
docker build -t actrix:latest .

# 启动服务
docker-compose up -d

# 查看日志
docker-compose logs -f actrix

# 停止服务
docker-compose down
```

### 部署
```bash
# 使用 deploy 工具生成配置
cargo run -p deploy -- config

# 生成 systemd 服务
cargo run -p deploy -- systemd -c config.toml

# 生成 docker-compose
cargo run -p deploy -- docker -c config.toml
```

---

## 自动化建议

### CI/CD 集成

在 GitHub Actions 中使用：

```yaml
# .github/workflows/security.yml
name: Security Check

on:
  push:
    branches: [ main ]
  schedule:
    - cron: '0 2 * * *'  # 每天凌晨 2 点

jobs:
  security:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - run: bash scripts/security-check.sh
```

### 定时备份

使用 cron 定时备份：

```bash
# 编辑 crontab
crontab -e

# 添加每天凌晨 3 点备份
0 3 * * * cd /opt/actrix && bash scripts/backup.sh
```

---

## 相关文档

- [配置指南](../docs/CONFIGURATION.md)
- [Docker 部署](../docs/DOCKER_DEPLOY.md)
- [部署工具](../deploy/README.md)
