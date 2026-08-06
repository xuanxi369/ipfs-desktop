# 文本编码约定

仓库中的源码、配置和 Markdown 文档统一使用 UTF-8。除 Windows PowerShell 脚本使用 CRLF 外，文本文件统一使用 LF。规则由根目录的 `.editorconfig` 和 `.gitattributes` 固定。

## Windows PowerShell 5

Windows PowerShell 5 的 `Get-Content` 默认按系统 ANSI 代码页读取无 BOM UTF-8，因此中文可能显示成乱码；这不代表文件已损坏。读取仓库文本时应显式指定编码：

```powershell
Get-Content -Encoding UTF8 README.md
Get-Content -Raw -Encoding UTF8 docs\PROJECT_ROADMAP.md
```

写入文本时也必须显式使用 UTF-8。推荐通过支持 `.editorconfig` 的编辑器修改文件，不要用未指定编码的 `Set-Content`、`Out-File` 或 `>` 覆盖中文文档。

## 提交前检查

```powershell
git diff --check
npm run typecheck
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
```

如果编辑器支持自动检测，请仍将项目编码固定为 UTF-8，不要依赖系统区域设置。
