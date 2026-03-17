# OmniIsle Demo

这个 configs 用于验证交互与执行链路，系统集成配置会同步到 `configs/app_configs.json`。

## 运行方式

在项目根目录执行：

```powershell
python configs/omniisle_demo.py
```

## 配置脚本按钮

脚本按钮来自配置文件 `configs/scripts_config.json`：

- `label`：按钮显示名
- `script`：脚本文件名（位于 `configs/scripts/` 下）

你新增脚本时，只需要：

1. 把脚本文件放到 `configs/scripts/`
2. 在 `configs/scripts_config.json` 中添加一项

## 你可以验证什么

- 顶部置顶悬浮窗口（模拟灵动岛收起态）
- 点击顶部收起/展开
- 点击“运行成功脚本”查看成功状态与日志
- 点击“运行失败脚本(演示)”查看错误状态与 stderr（这是故意失败的演示路径）
- 脚本通过 subprocess 静默执行，不额外弹出终端窗口

## 当前限制

- 动画是 Tkinter 版本，目的是验证流程，不是最终视觉质量。
- 没有脚本配置管理器（暂时写死两个脚本按钮）。
- 脚本配置管理器仍在逐步完善中（目前已支持基础运行与删除演示）。
