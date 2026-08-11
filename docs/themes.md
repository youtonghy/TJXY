# React 前台主题接口

TJXY 的主题系统用于切换普通用户前台 `/app/*` 的展示方式。管理后台本身不参与主题切换，
因此管理员始终可以使用稳定的后台界面恢复配置。当前内置 `classic`（经典）和 `cinema`
（影院）两个主题。

## 架构边界

主题是编译进 `admin/` 前端产物的 React 展示适配层，不是远程插件系统。服务端只保存主题 ID、
选项 schema 版本和 JSON 选项，不接收或执行 JavaScript、CSS 或外部资源地址。

主应用继续拥有路由、鉴权、数据请求、公告、AI 模型可用性、语言、品牌和页面内容。主题接收已经
整理好的导航、账户命令、明暗模式和内容节点，只决定 Shell 与登录页如何呈现。新增后台功能时，
应先在主应用中接线，再通过现有主题 props 暴露必要的展示数据；不要在每个主题中复制业务逻辑。

```text
GET /System/Settings
        |
        v
SystemLocaleProvider -> ClientThemeRuntime -> 静态 registry
                                                |-- classic Shell/LoginFrame
                                                `-- cinema Shell/LoginFrame
        |
        `-> 共享的鉴权、路由、页面和功能设置
```

## 管理与运行时流程

- 管理员在 `/admin/settings/theme` 选择主题并编辑该主题声明的选项。
- `GET /Admin/System/Theme` 返回当前主题、所有已保存主题配置和独立 revision。
- `PUT /Admin/System/Theme` 使用 revision 做 compare-and-swap；过期保存返回 `409 Conflict`。
- 切换主题时，其他主题的配置会保留，切换回来时继续使用原值。
- 保存成功后当前标签页立即广播配置；其他标签页和新会话通过公开设置重新读取。
- `/app/*` 遇到未知主题 ID、不支持的 schema 或无效选项时使用注册表默认值；未知主题回退到
  `classic` 并显示警告。
- 主题加载失败由边界错误页接管，不会执行来自数据库的代码。

主题运行时只在 `<html>` 上设置带前台作用域的 `data-tjxy-surface="client"`、
`data-client-theme`、`data-client-accent` 和 `data-client-density`。卸载时会恢复原属性和明暗模式，
主题 CSS 必须以这些前台属性作为作用域，不能影响 `/admin/*`。

## React 接口

注册表位于 `admin/src/client/themes/registry.tsx`，契约位于
`admin/src/client/themes/types.ts`。每个 `ClientThemeDefinition` 必须提供：

- 稳定、全小写的 `id`；允许字母、数字和连字符，最大 64 字符。
- 本地化名称与说明 key。
- 正整数 `schemaVersion`、`defaultOptions` 和 `normalizeOptions`。
- 声明式 `optionFields`，后台据此生成控件。
- 顶层声明的 `React.lazy` Shell 与 LoginFrame，以及同步的轻量 Preview。

`ThemeShellProps` 提供共享页面内容、已按可用功能过滤的导航、品牌、用户、公告、当前路径、
明暗模式和命令回调。`ThemeLoginFrameProps` 提供品牌、语言操作区、共享登录表单和主题选项。
主题组件不得自行请求管理员 API、读取凭据或重新实现路由权限。

## 选项与版本

选项必须是 JSON 对象。服务端限制单份选项为 16 KiB，并限制嵌套深度、条目数和字符串长度；
前端 `normalizeOptions` 只输出当前 schema 已知的键和值。选项 key 使用 camelCase。

当前内置选项：

| 主题 | schema | 选项 |
|---|---:|---|
| `classic` | 1 | `contentWidth`: `standard` 或 `wide` |
| `cinema` | 1 | `density`: `comfortable` 或 `compact`; `contentWidth`: `standard` 或 `wide`; `accent`: `crimson`、`gold` 或 `teal` |

兼容地新增可选字段时可保留 schema 版本并提供默认值。删除字段、改变含义或改变值类型时必须递增
`schemaVersion`，并在 `normalizeOptions` 中显式迁移仍受支持的旧版本。无法迁移的版本应返回当前
主题默认值，不要猜测数据。

## HTTP 契约

公开 `GET /System/Settings` 的 `Theme` 字段示例：

```json
{
  "Id": "cinema",
  "SchemaVersion": 1,
  "Options": { "density": "compact", "contentWidth": "wide", "accent": "teal" },
  "Revision": 4
}
```

管理员 `GET /Admin/System/Theme` 返回：

```json
{
  "ActiveThemeId": "cinema",
  "Configurations": [
    { "ThemeId": "classic", "SchemaVersion": 1, "Options": { "contentWidth": "standard" } },
    { "ThemeId": "cinema", "SchemaVersion": 1, "Options": { "density": "compact" } }
  ],
  "Revision": 4
}
```

管理员 `PUT /Admin/System/Theme` 只提交要启用的主题配置：

```json
{
  "ThemeId": "cinema",
  "SchemaVersion": 1,
  "Options": { "density": "compact", "contentWidth": "wide", "accent": "teal" },
  "Revision": 4
}
```

首次保存省略 `Revision`；之后必须发送最近读取的正整数 revision。管理员接口要求管理员 token，
缺少认证返回 `401`，普通用户返回 `403`，输入无效返回 `400`，revision 冲突返回 `409`。

## 新增主题

1. 在 `admin/src/client/themes/<id>/` 实现 Shell 与 LoginFrame，复用共享 props 和 HeroUI 组件。
2. 在 `registry.tsx` 顶层声明 lazy import，并注册 definition 与 Preview。
3. 为每个选项提供默认值、字段声明、严格归一化和中英文文案。
4. 将 CSS 限定在 `html[data-tjxy-surface='client'][data-client-theme='<id>']` 下。
5. 添加注册表归一化/降级测试、Shell 与登录页交互测试，并验证桌面和移动视口。
6. 更新本文件的内置选项表。部署包含新主题的前端产物后，管理员页面才会显示它。

移除主题前应先发布一个版本让管理员切换到仍受支持的主题。即使数据库仍保存旧 ID，客户端也会
回退到 `classic`，管理员页面会提示修复配置。

## 验证

```bash
npm --prefix admin run typecheck
npm --prefix admin run lint
npm --prefix admin test -- --run
npm --prefix admin run build
cargo test -p tjxy-db --test site_theme_settings_repository_contract
cargo test -p tjxy-db --test schema_contract site_theme_settings
cargo test -p tjxy-server --test browse_routes site_theme
```
