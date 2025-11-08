# Actrix 部署指南

本目录包含 Actrix 的部署脚本和配置文件。

## 文件说明

- `install.sh`: 自动化安装/卸载/更新脚本
- `actrix.service`: Systemd 服务配置文件
- `README.md`: 本文档

## 快速安装

### 1. 构建项目

```bash
cargo build --release
```

### 2. 运行安装脚本

```bash
sudo ./deploy/install.sh install
```

安装脚本会：
- 创建系统用户 `actrix:actrix`
- 安装二进制文件到 `/opt/actrix/`
- 复制配置文件到 `/opt/actrix/config.toml`
- 安装 systemd 服务

### 3. 配置服务

编辑配置文件：

```bash
sudo nano /opt/actrix/config.toml
```

关键配置项：
- `enable`: 服务启用位掩码
- `actrix_shared_key`: **必须修改默认值！**
- `bind.https`: HTTPS 证书和端口配置
- `bind.ice`: STUN/TURN 端口配置
- `turn.public_ip`: TURN 服务器的公网 IP
- `log_output`: 设置为 "file" 并启用 log_rotate

### 4. 启动服务

```bash
sudo systemctl enable actrix
sudo systemctl start actrix
sudo systemctl status actrix
sudo journalctl -u actrix -f
```

## 更新

```bash
git pull
cargo build --release
sudo ./deploy/install.sh update
```

## 卸载

```bash
sudo ./deploy/install.sh uninstall
```

完整文档请访问: https://github.com/actor-rtc/actrix
