# Writing Studio

AI-assisted writing studio built on Rinx.

## 功能

- 📝 文档管理：新建、编辑、删除文档
- ✨ AI 改写：选中文字，请求 AI 改写（支持更简洁、更正式、保留引用）
- 🔍 逐词 Diff 对比：左右对照看 AI 改写了什么
- 📤 发布：把文档发送到聊天、文章编辑器，或导出为文件
- 📜 历史记录：每次改写都有记录，可撤销

## 团队

- bluemsun-lin
- bluemsun-CK橙柚
- bluemsun-Endangered

## 环境要求

- Rinx 宿主：本仓库 main 分支 commit `da0c4bff`
- Rust 1.98.0+
- Windows 10/11

## 构建与运行

### 1. 克隆仓库

```bash
git clone https://github.com/hhyyzz-enda/bluemsun-2026AgentticApp.git
cd bluemsun-2026AgentticApp
```

### 2. 启动 Rinx

```bash
cargo run --locked
```

### 3. 打开写作工作室

在 Rinx 应用列表中找到 "Writing Studio" 并打开。

## Agent 任务演示

### 场景：写一篇短文，让 AI 帮忙润色

1. **用户输入**：在写作工作室新建文档，写一段初稿
2. **Agent 读取状态**：读取当前文档内容
3. **用户请求 AI 改写**：选中一段文字，选择"更简洁"或"更正式"
4. **Agent 提行动**：调用 LLM 生成改写结果
5. **人在回路**：用户在 Diff 对比页查看改写结果，确认接受或拒绝
6. **结果核验**：接受后文档更新，历史记录保存这次改写
7. **发布**：用户确认后，发送到聊天或导出为文件

### 授权与确认

- 所有 AI 改写都需要用户主动触发
- 改写结果需要用户确认才会应用
- 删除文档前会弹出确认窗口

### 失败处理

- AI 请求失败时显示错误提示，不影响原有文档
- 网络错误时自动重试或提示用户检查网络

## 权限声明

| 权限 | 用途 |
|------|------|
| `storage` | 本地保存文档和改写历史 |
| `matrix.send_message` | 把文档发送到聊天 |

## 隐私说明

- 所有文档数据都保存在本地，不上传服务器
- AI 改写请求会发送选中的文字到 LLM 服务（智谱），不发送完整文档
- 不收集任何用户个人信息

## 真实服务 vs 本地数据

| 功能 | 数据来源 |
|------|----------|
| 文档存储与编辑 | 本地存储，无外部依赖 |
| AI 改写 | 接入智谱（Zhipu）真实 LLM 服务 |
| Matrix 消息发送 | 接入 Rinx 内置 Matrix 客户端 |
| Diff 对比 | 本地计算，无外部依赖 |
| 历史记录 | 本地存储 |

## 截图

见 `bundle/screenshots/`

## 演示视频

见 `bundle/基础功能演示.mp4`

## 许可证

Apache 2.0
