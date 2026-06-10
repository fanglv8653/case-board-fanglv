read_kb_file — 读本地知识库内某个文件的完整内容(防路径穿越;支持 offset + length 分段)

适用场景:
- `search_local_kb` 命中后,挑出最相关 1-2 个文件用本工具拿全文
- 用户直接给出 KB 内某个相对路径(`wiki/sources/X.md`),想看完整内容
- 接到任务后想直接读 KB 入口文件(`wiki/index.md` / `gap-log.md`)了解整体结构
- 元典缓存命中后想看完整缓存文件(`raw/yuandian-cache/SEARCH-xxx.md`)

不适用:
- 不知道路径 → 先用 `search_local_kb` 拿候选
- 读案件文档 → 用 `read_case_doc(doc_id)`
- 读案件**外部**任意文件(KB 之外的)→ 不允许,本工具受 KB 根目录约束

输入字段:
- relative_path: 必填,KB 内相对路径(**不能**以 `/` 开头,**不能**含 `..`,**不能**为绝对路径,工具会拒并报 PathEscape)
  - 好例子:`wiki/sources/民法典-合同篇.md`、`raw/notes/2024-合同解除研究.md`
  - 坏例子:`/etc/passwd`、`../../../something`(被防穿越拒)
- offset: 可选,从第几个 char 开始(默认 0)
- length: 可选,读多少 char(默认 10000)

注意事项:
- **本工具不消耗元典积分**(本地文件读)
- 安全约束(`KbError::PathEscape`):路径必须 canonicalize 后仍在 kb_root 内
- 文件大小约束:**单文件上限 5MB**,超出报 `FileTooBig`(法律 KB 单文件通常不会超过 100KB)
- 二进制文件检测:open 后读头 512 字节,含 NUL 字符报 `BinaryFile`(KB 文件应该都是 markdown / txt)
- 返回结构:文件完整内容字符串(已按 offset + length 截取)
- KB 未启用(用户没填 local_kb_root)时,本工具直接报错 — 上层 agent_loop 应该在 `search_local_kb` 返回空时就放弃本路径,不应该再调 read
- 引用 KB 内容时,**必须**加入 `<CITATIONS>`:type = `"kb_local"`,title = 文件名(去后缀),source = relative_path
