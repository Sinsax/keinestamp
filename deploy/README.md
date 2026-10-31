# serve 部署文件

本目录包含 Linux / systemd 的部署模板：

- `keinestamp.service` —— systemd unit 模板

完整的服务端部署说明（程序目录、serve.toml、反向代理、令牌、loreserver、故障排查）
见 **[docs/server-guide.md](../docs/server-guide.md)**。

快速用法：

```bash
cp deploy/keinestamp.service ~/.config/systemd/user/
# 按注释修改 ExecStart / WorkingDirectory / KEINESTAMP_BASE_DIR / ReadWritePaths
systemctl --user daemon-reload
systemctl --user enable --now keinestamp.service
```
