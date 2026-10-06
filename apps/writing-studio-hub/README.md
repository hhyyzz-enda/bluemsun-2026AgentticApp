# 写作工作室 · App Hub 版（writing-studio-hub）

把 Rinx 原生小程序 `Rinx/apps/writing-studio/`（Rust + Makepad 原生代码）按 App Hub
上架要求重写为**独立 OctoScript 脚本小程序**：所有 UI 与逻辑复刻进
`bundle/main.splash`，不含任何原生代码。原原生版本目录未做任何改动。

## 目录

```
writing-studio-hub/
  README.md            本说明
  bundle/              提交 App Hub 的全部内容
    manifest.json      身份、能力、配额（id: bluemsun.writing-studio）
    listing.json       商店展示信息
    main.splash        全部功能（约 1400 行脚本）
    assets/icon.svg    图标（沿用原生版）
    screenshots/       商店截图（沿用原生版三张）
```

## 功能对照（与原生版一致）

| 功能 | 原生版 | App Hub 版 |
|---|---|---|
| 文库：草稿卡片、预览、段数/版本/最后编辑 | PortalList | on_render 列表 |
| 每篇文档最多 3 条任务历史（状态徽章、可跳审阅） | ✔ | ✔ |
| 新建 / 编辑文档（标题 + 正文，段落空行分隔） | ✔ | ✔ |
| 选中文字改写 | 光标选区，无选区取末尾 200 字 | 段落点选，默认最后非空段 |
| 约束开关：更简洁 / 更正式 / 保留引用 | ✔ | ✔ |
| 三级引擎：Octos 宿主 → 直连 LLM → Mock | ✔ | ✔（`octos.turn.start` / `net` + `llm.json` / 内置规则） |
| 准备中卡片显示当前引擎 | ✔ | ✔ |
| 请求含「失败」注入失败；失败卡可重试 | ✔ | ✔ |
| 提案 600 秒过期；过期卡可重新生成 | ✔ | ✔ |
| 左右对照 + 提案可编辑 + 逐词 diff（＋/− 摘要） | ✔ | ✔（LCS，中文按字、ASCII 按词） |
| 引用标记 `[^n]` 检测：丢失阻断确认、保留列出、长文提示需来源 | ✔ | ✔ |
| 确认页：目标摘要 + 修改后全文预览 | ✔ | ✔ |
| 应用双重校验（文档版本 + 段落快照），变更即阻断 | ✔ | ✔ |
| 应用幂等、撤销恢复原文 | ✔ | ✔ |
| 发布四目的地，各自独立确认 | ✔ | ✔（聊天走 `matrix.send_message`；图文工作室落到本地排版库；导出文件真实写入应用存储；分享链接演示） |
| 从图文工作室拉回继续改写（版本对齐） | ✔ | ✔（对齐 / 新建导入） |
| 删除文档 / 任务：确认弹窗 + 级联清理（任务、日志、撤销、排版草稿） | ✔ | ✔ |
| 决定日志 decisions.jsonl（读取→提议→确认→执行→核验） | ✔ | ✔ |
| 首次启动内置演示文档 | ✔ | ✔（同一份示例文案） |

## 密钥与隐私（与原生版同红线）

- 应用**零密钥**：优先走 `octos.turn.start`，密钥由宿主托管；
- 宿主无 Octos 服务时，读取应用存储内的 `llm.json`
  （`{"base_url","api_key","model"}`，如智谱 `https://open.bigmodel.cn/api/paas/v4`）直连；
  该文件由使用者自行放入应用数据目录，不进 bundle、不进仓库；
- 都没有则回退内置 Mock，演示不中断；
- 改写只发送选中段落，不发送完整文档。

## 上架流程

```sh
hub stamp bundle          # 每次改动后重算摘要
hub check bundle          # 门控（开发期 --allow-unsigned）
card-host --bundle bundle --app-data .local-state --allow-unsigned   # 本地运行
```

签名与提交见 `OctoSense-App-Hub/docs/PUBLISHING.md`。

## 已知差异（沙箱所限）

- 发布到聊天 / 拉回功能依赖宿主提供 `matrix.*` 服务（Rinx 小程序宿主提供；
  card-host 无此服务时会显示明确的降级提示，流程与日志不受影响）；
- 「图文工作室」为应用存储内的本地排版库（`articles.json`），不复用 Rinx 原生
  文章编辑器；拉回的「分叉」情形简化为按 `article_id` 对齐覆盖。
