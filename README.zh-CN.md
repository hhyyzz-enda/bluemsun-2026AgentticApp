# Rinx

[English](README.md) | 简体中文

Rinx 是原生 Matrix 客户端，包含聊天、联系人、发现、朋友圈和文章编辑器，支持独立运行及作为 OctoSense 原生模块运行。完整构建与兼容性说明见 [英文文档](README.md)。

---

## 🏆 参赛作品：Writing Studio

**本仓库为 Agentic App 黑客松参赛作品，应用为 Writing Studio（写作工作室）。**

- 📖 应用说明：[apps/writing-studio/README.md](apps/writing-studio/README.md)
- 📸 截图：[apps/writing-studio/bundle/screenshots/](apps/writing-studio/bundle/screenshots/)
- 🎬 演示视频：[apps/writing-studio/bundle/基础功能演示.mp4](apps/writing-studio/bundle/基础功能演示.mp4)
- 🏷️ 提交版本：tag `v1.0.0`

---

## 本地编译与运行

```bash
# 快速检查（只编译不链接，报错最快）
cargo check --locked

# 编译并运行（首次链接要 10-20 分钟，之后增量编译很快）
cargo run --locked
```

## 内置应用

`system-apps.json` 定义随 Rinx 发布的原生和 OctoScript 应用。原生文章编辑器位于 `apps/article-editor/`；共享文档和 Makepad 控件库保留在 `crates/`。

独立版由 Rinx 管理 Octos 运行时和提供商配置；托管版使用 OctoSense 注入的服务、共享内核与配置。小程序通过受限的宿主接口访问 Matrix 和 Octos，不启动自己的内核。

应用目录构建验证、开发和发布流程见 [应用开发指南](apps/README.zh-CN.md)；架构决策见 [ADR 0008](docs/adr/0008-rinx-system-app-catalog.md)。
