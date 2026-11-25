# 小鸿 M2 API 参考

## 调用约定

ArkTS N-API 模块提供八个异步方法：`init`、`addDocuments`、`search`、`searchScoped`、`resolve`、`resolveScoped`、`styleProfile`、`removeDocument`。每个方法只接收一个 JSON 字符串，返回 `Promise<string>`。C ABI 对应每方法一个 JSON 参数和一个 JSON envelope 字符串返回值。输入上限为 32 MiB。

```json
{"ok":true,"data":{}}
{"ok":false,"error":{"code":"INVALID_REQUEST","message":"..."}}
```

N-API 参数类型/长度、内存分配或异步调度失败会同步抛错或 reject；Rust 业务错误作为上述 `ok:false` envelope 返回。N-API 不记录查询、文档、向量或引用内容。

## `init`

```json
{"config":{"chunkTargetChars":1024,"chunkOverlapChars":96,"embeddingDimension":0,"minimumChunkChars":24}}
```

`config` 可省略。各字段分别默认为 1024、96、0、24。target/minimum 必须大于零，minimum 不得大于 target，overlap 必须小于 target。默认安装维度为 0 的 `NoopEmbedder`，即 keyword-only。成功的 `data` 为 `null`。C ABI 不加载模型。Rust 宿主可通过 `install_engine` 注入实现 `Embedder` 的向量编码器；非零配置维度必须与编码器维度相同。

## `addDocuments`

```json
{"documents":[{"documentId":"N1","revision":3,"title":"线代笔记","course":"线性代数","text":"特征值与特征子空间……"}]}
```

批次必须非空、ID 不得重复，ID/title 不得为空；同一 ID 的 revision 必须严格递增。成功 `data` 为 `{documents,chunks,embeddingDimension}`。写入按整批准备后提交，输入无效或 embedding 失败不会部分提交。索引驻留进程内存，进程退出即丢失。

## `search`

```json
{"query":"特征值 几何重数","courseFilter":"线性代数","topK":5,"rrfK":60,"vectorWeight":1.0,"keywordWeight":1.0}
```

`query` 不得为空；`courseFilter` 可省略；`topK` 范围为 1..50，默认 5；`rrfK` 范围为 1..10000，默认 60；权重默认 1，须有限、非负且不能同时为零。关键词算法是确定性基线：ASCII 字母数字分组，汉字单字及相邻双字，并非完整中文分词器。默认无 embedding 时只用关键词通道，并返回 keyword-only warning。注入 embedding 后，负余弦相似度按零处理。每通道按 `weight / (rrfK + rank)` 融合，按总分降序、chunk ID 升序排序。

成功 `data` 是 `{schemaVersion,query,hits,warnings}`。每个 hit 包含 citation、score、vectorRank 和 keywordRank；未参与的 rank 序列化为 `null`。citation 的 `sourceStart`/`sourceEnd` 是原文 UTF-8 字节半开区间，不是 ArkTS UTF-16 下标。命中是候选证据，不是生成答案。

## `searchScoped`

```json
{"request":{"query":"特征值","topK":5},"allowedDocumentIds":["N1","N2"]}
```

`request` 字段是完整 SearchRequest（其余字段按默认值处理），`allowedDocumentIds` 必须提供，不能含空白 ID。空数组选择零篇文档。范围在排序与 topK 前应用，并与 `courseFilter` 取交集。此字段是调用方提供的选择范围，不认证调用者、不构成授权边界；上层必须自行实施身份和权限校验。

## `resolve` 与 `resolveScoped`

`resolve` 请求：

```json
{"documentId":"N1","revision":3,"chunkId":"N1:r3:c0001"}
```

`resolveScoped` 请求：

```json
{"reference":{"documentId":"N1","revision":3,"chunkId":"N1:r3:c0001"},"allowedDocumentIds":["N1"]}
```

成功 `data` 为当前索引中的完整 Citation；未知文档、删除、过期 revision、未知 chunk 或 scoped 范围不包含该文档时返回 `null`。正文从当前索引读取，不信任调用方提交的 citation 正文。

## `styleProfile`

```json
{"allowedDocumentIds":["N1","N2"]}
```

字段必须提供；空范围返回默认空统计。只汇总允许范围内当前存在文档的结构信息，返回 `headingLevels`、`orderedList`、`bullets`、`averageParagraphLength`。段落长度按 Unicode grapheme 数平均计算；标题、列表和代码块不计入段落。该接口不返回笔记正文、不做语义推断；范围不是认证或授权机制。

## `removeDocument`

字段名为 camelCase：`{"documentId":"N1"}`。成功 `data` 是布尔值，表示当前是否删除了文档。重复删除返回 false；删除后仍保留 revision 高水位，避免旧 revision 重新写入。

## 错误码

`INVALID_JSON`、`INVALID_REQUEST`、`DOCUMENT_NOT_FOUND`（当前保留码）、`EMBEDDING_FAILED`、`VECTOR_DIMENSION_MISMATCH`、`INDEX_LOCKED`、`NULL_INPUT`。C ABI 空指针、无效 UTF-8、过长输入或可捕获 panic 会尝试返回错误 envelope；若系统分配失败则返回空指针。C ABI 无法验证非空指针是否指向有效、可读且 NUL 结尾的内存。

## C ABI 所有权

`include/m2_ffi.h` 声明八个 `m2_*_json` 操作和 `m2_free_string`。输入为借用的 NUL 结尾 UTF-8 字符串；每次成功分配的返回字符串必须且只能调用 `m2_free_string` 释放，不得调用 C `free`。每个 FFI 操作捕获 Rust panic；Rust panic 不得跨越 C ABI 边界。

## 限制与验证边界

索引仅为内存实现，无持久化、ArkData adapter、身份验证或授权。默认 keyword-only；`Embedder` 是供 Rust 宿主注入的扩展点，仓库不包含或下载模型。scoped 方法只过滤调用方指定的 ID。styleProfile 只提供结构统计。OpenHarmony 构建依赖匹配版本的 SDK/NDK、Rust target 和 linker；本机测试不等于 OpenHarmony 交叉构建或真机验证。# API 参考

## 调用约定

ArkTS 模块导出八个异步方法：`init`、`addDocuments`、`search`、`searchScoped`、`resolve`、`resolveScoped`、`styleProfile`、`removeDocument`。每次调用只传一个 UTF-8 JSON 字符串，返回 `Promise<string>`；scoped 范围也必须放在该 JSON 对象中。输入最大 32 MiB。N-API 的参数类型或长度错误同步抛出；业务错误通过 envelope 返回。

```json
{"ok":true,"data":{}}
{"ok":false,"error":{"code":"INVALID_REQUEST","message":"..."}}
```

业务错误码：`INVALID_JSON`、`INVALID_REQUEST`、`DOCUMENT_NOT_FOUND`（预留）、`EMBEDDING_FAILED`、`VECTOR_DIMENSION_MISMATCH`、`INDEX_LOCKED`、`NULL_INPUT`。不记录 query、文档正文、向量或 citation。

## 初始化：`init`

```json
{"config":{"chunkTargetChars":1024,"chunkOverlapChars":96,"embeddingDimension":0,"minimumChunkChars":24}}
```

`config` 可省略并采用示例默认值。`minimumChunkChars` 与 `chunkTargetChars` 必须大于零，minimum 不得大于 target，overlap 必须小于 target。成功 `data` 为 `null`。默认 Rust 入口安装维度为 0 的 `NoopEmbedder`，只执行关键词检索。配置非零维度需 Rust 宿主通过 `install_engine` 注入维度匹配的实现；当前 C ABI init 不加载或配置模型。

## 写入文档：`addDocuments`

```json
{"documents":[{"documentId":"N1","revision":3,"title":"线代笔记","course":"线性代数","text":"特征值与特征子空间……"}]}
```

ID/title 不得为空，批次不得为空或含重复 ID；同一文档 revision 必须严格递增。成功 data 为 `{documents,chunks,embeddingDimension}`。索引仅驻留内存，没有持久化实现。

## 搜索：`search`

```json
{"query":"特征值 几何重数","courseFilter":"线性代数","topK":5,"rrfK":60,"vectorWeight":1.0,"keywordWeight":1.0}
```

`query` 不能为空；`courseFilter` 可省略；`topK` 默认 5，范围 1..50；`rrfK` 默认 60，范围 1..10000；权重默认 1，必须有限、非负且不同时为零。关键词通道使用拉丁数字分组 token 与汉字单字/相邻双字。若宿主注入向量编码器，负相似度按零参与排序。每个通道按 `weight / (rrfK + rank)` 计分；按总分降序、chunkId 升序稳定排序。

成功 data 示例：

```json
{"schemaVersion":1,"query":"特征值","hits":[{"citation":{"documentId":"N1","revision":3,"title":"线代笔记","course":"线性代数","chunkId":"N1:r3:c0001","sourceStart":0,"sourceEnd":32,"text":"特征值与特征子空间……"},"score":0.032,"vectorRank":null,"keywordRank":1}],"warnings":["No embedding provider is configured; results use the keyword channel only."]}
```

`sourceStart` / `sourceEnd` 是原始修订 UTF-8 字节偏移的半开区间 `[start,end)`，不是 ArkTS UTF-16 下标。无编码器时 `vectorRank` 为 null 并返回 keyword-only warning。命中只是候选，不是生成答案。

## 融合搜索：`federatedSearch`

本机索引始终是默认来源 `"local"`。Rust 宿主可通过 `register_provider` 注入实现 `SearchProvider` trait 的其他源（如其他 App、系统搜索或互联网服务）。请求：

```json
{"query":"几何重数","sources":[],"topK":6,"rrfK":60}
```

`sources` 省略或为空表示本机索引加全部已注册源；提供 ID 列表则只选这些源。每个源先内部排序（源内 rank 从 1 开始），再按 `1/(rrfK + rank)` 跨源融合，总分降序、sourceId、reference 稳定排序后截断到 `topK`。

成功 `data`：

```json
{"schemaVersion":1,"query":"几何重数","hits":[{"sourceId":"local","title":"线代复习","reference":"DEMO-LA-001:r1:c0001","snippet":"几何重数等于特征子空间的维数……","providerScore":0.016,"fusedScore":0.016,"sourceRank":1}],"warnings":[]}
```

- `reference` 对本地命中是可回传 `resolve` 的 `chunkId`；外部源由 provider 自行定义 URI/引用。
- 单个源失败或返回错误时进入 `warnings`，不影响其他源；融合结果仍是候选证据，不是生成答案。
- 本 crate 不实现外部源、不发起网络/跨应用调用；宿主必须在接入与出端前校验数据等级、用户许可和网络可达性。

## 限范围搜索：`searchScoped`

```json
{"request":{"query":"特征值","topK":5},"allowedDocumentIds":["N1","N2"]}
```

`request` 是 `search` 请求对象；`allowedDocumentIds` 必须提供，ID 不能是空白。空数组选择零篇文档。范围在排名和 topK 之前应用。该字段只约束本次选择，不能认证调用方，也不能替代应用权限校验。

## 引用回链：`resolve` / `resolveScoped`

无范围请求：`{"documentId":"N1","revision":3,"chunkId":"N1:r3:c0001"}`。成功 data 是当前索引中的 `Citation`；文档删除、修订过期或 chunk 未知时为 `null`。

范围版请求：`{"reference":{"documentId":"N1","revision":3,"chunkId":"N1:r3:c0001"},"allowedDocumentIds":["N1"]}`。ID 不在范围内时 data 为 `null`。正文从当前索引重取，不信任调用方传入的 citation 内容。

## 风格统计：`styleProfile`

```json
{"allowedDocumentIds":["N1","N2"]}
```

成功 data 包含 `headingLevels`、`orderedList`、`bullets`、`averageParagraphLength`，只汇总指定且当前存在的文档结构，不返回原文段落。空范围返回空统计；范围只是样本选择。

## 删除：`removeDocument`

请求字段使用 camelCase：`{"documentId":"N1"}`。data 为布尔值，表示是否删除了当前索引文档；重复删除返回 false。删除后保留 revision 高水位，因此旧 revision 不能重新写回。

## C ABI

`include/m2_ffi.h` 提供 `m2_init_json`、`m2_add_documents_json`、`m2_search_json`、`m2_search_scoped_json`、`m2_resolve_json`、`m2_resolve_scoped_json`、`m2_style_profile_json`、`m2_remove_document_json`。每个函数接受一个 NUL 结尾 UTF-8 JSON 指针并返回 envelope。超过 32 MiB、空指针或非法 UTF-8 输入会返回错误 envelope（若分配成功）；Rust panic 会在边界捕获。

返回字符串由 Rust 分配，调用方必须且只能使用 `m2_free_string` 释放，不得使用 C `free`。输入指针须指向有效且 NUL 结尾的可读内存，否则 C ABI 无法检测其有效性。

## 实现与安全限制

索引为进程内存，不具备持久化；没有真实 embedding 模型或模型权重。未实现用户认证、授权、ArkData 持久化、加密或跨进程安全边界。OpenHarmony 构建依赖外部 SDK/NDK；主机测试不等于设备构建或真机验证。关键词切分是基线实现，不是完整中文分词器。# API Reference

## ArkTS native module

```ts
import rag from 'libxiaohong_rag.so';
await rag.init(configJson: string): Promise<string>;
await rag.addDocuments(requestJson: string): Promise<string>;
await rag.search(requestJson: string): Promise<string>;
await rag.removeDocument(requestJson: string): Promise<string>;
```

Each call takes one UTF-8 JSON string (max 32 MiB) and resolves to an envelope:

```json
{"ok":true,"data":{}}
{"ok":false,"error":{"code":"INVALID_REQUEST","message":"..."}}
```

Business failures resolve as `ok:false`; only argument/native transport failures reject/throw. Inputs and outputs remain local to the process. No query or document logging is performed.

## Initialization

Input `InitRequest`:

```json
{"config":{"chunkTargetChars":1024,"chunkOverlapChars":96,"embeddingDimension":0,"minimumChunkChars":24}}
```

All fields are optional and use these defaults. `embeddingDimension:0` means keyword-only mode. To enable vector retrieval, the embedding provider must report the configured dimension.

Success data: `null`.

## Add / replace documents

Input:

```json
{"documents":[{"documentId":"N1","revision":3,"title":"线代笔记","course":"线性代数","text":"特征值与特征子空间……"}]}
```

- `documentId`: stable app-side note identifier.
- `revision`: monotonically increasing revision assigned by the application.
- Re-adding the same ID replaces its previous in-memory revision.
- Text is split by newline/sentence boundaries, then Unicode grapheme boundaries. Chunks preserve UTF-8 byte offsets into the exact source revision.
- Empty document arrays and empty IDs/titles are rejected.

Success data: `IndexStats { documents, chunks, embeddingDimension }`.

## Search

Input:

```json
{"query":"特征值 几何重数","courseFilter":"线性代数","topK":5,"rrfK":60,"vectorWeight":1.0,"keywordWeight":1.0}
```

- `query` must be non-empty.
- `courseFilter` is optional and filters the allowed document set.
- `topK` defaults to 5 and is limited to 1..50.
- RRF `k` defaults to 60; weights default to 1.0 and must be finite/non-negative.
- Keyword channel uses grouped ASCII alphanumeric tokens and Han unigrams/bigrams.
- Vector channel uses cosine similarity of the injected `Embedder`; negative similarity is clamped to zero for ranking.
- Results sort by fused score descending, then stable chunk ID ascending.

For channel rank `r`, RRF contributes `weight / (k + r)`. Missing/non-matching channel hits contribute zero. Results contain candidates only; they are not generated answers.

Success data:

```json
{
  "schemaVersion":1,
  "query":"特征值 几何重数",
  "hits":[{
    "citation":{
      "documentId":"N1","revision":3,"title":"线代笔记","course":"线性代数",
      "chunkId":"N1:r3:c0001","sourceStart":0,"sourceEnd":32,"text":"特征值与特征子空间……"
    },
    "score":0.032,
    "vectorRank":1,
    "keywordRank":2
  }],
  "warnings":[]
}
```

`sourceStart/sourceEnd` are half-open UTF-8 byte offsets into the original document revision. A UI that indexes UTF-16 must convert these offsets before selecting text.

When `embeddingDimension == 0`, vector ranks are absent and response warnings report keyword-only fallback.

## Remove document

Input: `{"documentId":"N1"}`. Success data is a boolean indicating whether an indexed note was removed.

## Error codes

`INVALID_JSON`, `INVALID_REQUEST`, `DOCUMENT_NOT_FOUND` (reserved), `EMBEDDING_FAILED`, `VECTOR_DIMENSION_MISMATCH`, `INDEX_LOCKED`, `NULL_INPUT`.

## C ABI

```c
char* m2_init_json(const char* json);
char* m2_add_documents_json(const char* json);
char* m2_search_json(const char* json);
char* m2_remove_document_json(const char* json);
void m2_free_string(char* value);
```

Input is borrowed NUL-terminated UTF-8; output is owned and must be freed exactly once by `m2_free_string`. Null input returns an error envelope if allocation succeeds. The C++ N-API wrapper runs core calls on async work, resolves the same JSON envelope, and must free every returned C string.

## Implementation limits

- Current reference engine is in-memory; persistence/vector storage is an adapter responsibility.
- Keyword tokenizer is a deterministic baseline, not a full Chinese word segmenter.
- `Embedder` is an injection seam. No model is bundled or downloaded.
- No RRF quality, latency, or device performance claim is made without a target-device benchmark.
