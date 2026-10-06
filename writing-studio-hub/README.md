# writing-studio-hub

「写作工作室」的 App Hub 提交版。将 Rinx 原生小程序 `apps/writing-studio/native`（Rust + Makepad）的全部功能复刻为 OctoScript 脚本应用，可上传至 OctoSense App Hub。原版原生代码不受影响，本目录完全独立。

## 目录结构

```
writing-studio-hub/
├── bundle/                  # 提交给 App Hub 的 bundle
│   ├── manifest.json        # 应用清单（id: bluemsun.writing-studio, v1.0.0）
│   ├── listing.json         # 商店展示信息
│   ├── main.splash          # OctoScript 主程序（完整复刻原生功能）
│   ├── assets/icon.svg      # 应用图标
│   └── screenshots/         # 商店截图（01 文库 / 02 编辑器 / 03 审阅）
└── build/                   # 校验产物（review.json 等，不提交）
```

## 功能（与原生版对齐）

- 文库：文档列表、新建 / 重命名 / 删除、本地持久化
- 编辑器：正文编辑、字数统计、自动保存
- AI 写作助手：选中文段 → 智谱 GLM（open.bigmodel.cn）改写 / 扩写 / 润色，流式回填
- 审阅模式：逐条接受 / 拒绝 AI 修改建议
- Octo 联动：通过 `octos.turn.start` 发起对话回合，`matrix.send_message` 推送结果通知

## 能力声明

| 能力 | 用途 |
|---|---|
| `storage`（8 MiB） | 文档与设置本地持久化 |
| `net` → `open.bigmodel.cn` | 调用智谱 GLM 接口，仅发送用户选中的段落 |
| `octos.turn.start` | 发起 Octo 对话回合 |
| `matrix.send_message` | 向用户推送写作结果通知 |

## 本地校验

```bash
HUB=/path/to/OctoSense-App-Hub/target/release/hub.exe
"$HUB" stamp bundle
"$HUB" check bundle --allow-unsigned
"$HUB" scan bundle --packet build/review.json
```

首次提交未签名，需加 `--allow-unsigned`。

## 提交

- 仓库：`hhyyzz-enda/bluemsun-2026AgentticApp`
- Tag：`writing-studio-hub-v1.0.0`
- Bundle 路径：`writing-studio-hub/bundle/`
- 在 `OctoSense-org/OctoSense-App-Hub` 开 issue，标题 `Submit bluemsun.writing-studio 1.0.0`，正文附仓库、tag、commit SHA、bundle 路径、`unsigned`、完整 `hub check` 输出与 `hub scan` 审查问题回答。
