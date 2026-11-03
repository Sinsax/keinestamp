# serve deployment files

**English** | [简体中文](README.zh-CN.md)

This directory contains deployment templates for Linux / systemd:

- `keinestamp.service` — systemd unit template

Full server deployment instructions (program directory, serve.toml, reverse proxy, tokens,
loreserver, troubleshooting) are in the
**[server guide](../docs/server-guide.md)**.

Quick usage:

```bash
cp deploy/keinestamp.service ~/.config/systemd/user/
# edit ExecStart / WorkingDirectory / KEINESTAMP_BASE_DIR / ReadWritePaths as noted in the comments
systemctl --user daemon-reload
systemctl --user enable --now keinestamp.service
```
