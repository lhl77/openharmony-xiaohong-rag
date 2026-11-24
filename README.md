# 小鸿 M2 本地检索引擎

独立 Rust crate，为端侧笔记提供分块、关键词检索、可注入向量编码器、RRF 排名和引用回链。C ABI 与 OpenHarmony N-API 遵循每个方法一个 JSON 字符串的契约。

## 功能

- 文档以 `documentId + revision` 标识；新修订替换旧索引，删除移除当前文档并保留 revision 高水位。
- Unicode grapheme 感知分块，保留原文 UTF-8 byte range；关键词基线支持 ASCII 字母数字 token、汉字单字和相邻双字。
- `Embedder` trait 允许 Rust 宿主注入向量编码器；仓库不包含或下载模型。
- `searchScoped`、`resolveScoped`、`styleProfile` 接受调用方提供的 `allowedDocumentIds`。空集合不选择任何文档；这不是身份验证或授权。
- 双通道 RRF 返回候选 Citation；`resolve` 可从当前索引重新取得引用内容。
- 默认 `NoopEmbedder` 的维度为 0，搜索只用 keyword 通道并在响应中发出 warning。

## 实现边界

- 索引仅在进程内存中，进程退出即丢失；没有磁盘持久化或 ArkData adapter。
- 默认没有真实 embedding。启用向量通道需 Rust 宿主注入 `Embedder`，并让其维度与配置一致；C ABI/N-API init 不加载模型。
- 关键词实现是确定性基线，不是完整中文分词器；检索结果是候选证据，不等于生成答案。
- 不提供用户身份验证、授权、跨进程安全边界、云服务或个人数据脱敏。`allowedDocumentIds` 只是选择过滤，上层必须自行实施访问控制。
- `styleProfile` 仅汇总标题级别、列表标记和正文段落平均 grapheme 长度等结构统计，不返回笔记正文，也不推断语义风格。
- 已提供 N-API 和交叉构建入口，但本仓库环境没有 OpenHarmony SDK/NDK，无法在此验证 OHOS target 链接或真机运行。

## 目录

```
src/                      索引、分块、tokenizer、契约类型及 C ABI
include/m2_ffi.h          公开 C 头文件
napi/napi_init.cpp        异步 OpenHarmony N-API 模块
arkts/                    ArkTS 类型声明与调用示例
tests/                    Rust 回归及 C ABI smoke 测试
scripts/                  OpenHarmony 交叉构建与 C smoke 入口
```

## 本机验证

```sh
cargo test --locked
cargo check --locked
bash scripts/test-ffi-c.sh
```

C smoke 会链接 cdylib 并真实调用 FFI 操作和错误路径。OpenHarmony 构建需配置安装好的 SDK/NDK、Rust target 与 linker：`bash scripts/build-ohos.sh arm64-v8a` 或 `bash scripts/build-ohos.sh x86_64`。主机测试不代表交叉构建或设备验证。

## API 与许可

N-API 提供 `init`、`addDocuments`、`search`、`searchScoped`、`resolve`、`resolveScoped`、`styleProfile`、`removeDocument` 八个异步方法，每个方法接收一个 JSON 字符串。完整 schema、envelope、错误码和 C ABI 所有权规则见 [API.md](API.md)。许可为 MIT，见 [LICENSE](LICENSE)。# 小鸿 M2 本地检索引擎

独立 Rust crate，为端侧笔记提供分块、关键词检索、可选的外部向量编码器接入、RRF 排名和引用回链。C ABI 与 OpenHarmony N-API 使用每个方法一个 JSON 字符串的统一契约。

## 功能

- 文档以 `documentId + revision` 标识；重建修订会替换该文档旧索引，删除文档会移除对应块。
- Unicode 感知切分，保留原文 UTF-8 byte range；中文关键词索引包含单字和相邻双字，拉丁数字按 token 匹配。
- 向量召回由 `Embedder` trait 注入，不绑定某一模型或 NPU。
- `searchScoped`、`resolveScoped` 和 `styleProfile` 接收调用方提供的允许文档 ID 集合。空集合不返回文档；这是选择约束，不是认证或安全边界。
- 双通道排名用 RRF 融合，返回 Top-K `Citation`；每条命中可回链原文、修订、课程及区间。
- 未配置 embedding 时仅走关键词通道，并在结果 warnings 明确提示。

## 能力边界

- 索引只存在进程内存，进程退出即丢失；没有磁盘持久化或 ArkData 仓储实现。
- 当前 crate 不包含、不下载也不调用真实 embedding 模型。默认 `NoopEmbedder` 维度为 0，始终是关键词模式；需要向量检索时，Rust 宿主须注入真实实现并匹配配置维度。
- 当前关键词实现是确定性基线，不是完整中文分词器；检索结果只是候选证据。
- 不提供用户身份验证、授权、跨进程 IPC、云服务或个人数据脱敏。上层必须自行实现认证和访问控制，不能把 `allowedDocumentIds` 当权限系统。
- N-API 和交叉构建源文件已提供；是否能链接及运行取决于 OpenHarmony SDK/NDK、系统版本和设备。本项目没有声称完成真机验证。

## 目录

```
src/                      索引、分块、tokenizer、契约类型及 C ABI
include/m2_ffi.h          公开 C 头文件
napi/napi_init.cpp        异步 OpenHarmony N-API 模块
arkts/                    ArkTS 类型声明与调用示例
tests/                    Rust 回归、C ABI 集成和独立 C 冒烟程序
scripts/                  OpenHarmony 交叉构建与 C 冒烟入口
```

## 本机验证

```sh
cargo test --locked
cargo check --locked
bash scripts/test-ffi-c.sh
```

C ABI 冒烟程序会编译并链接 cdylib 后实际调用 init、add、scoped search 和 remove。设备构建需配置 OpenHarmony SDK/NDK 的 `OHOS_NDK_HOME` 及对应 Rust target：`bash scripts/build-ohos.sh arm64-v8a` 或 `bash scripts/build-ohos.sh x86_64`。本机测试不等于设备构建或真机验证。

## API

ArkTS 调用顺序：`init` → `addDocuments` → `search`，删除时调用 `removeDocument`。每个方法只接收一个 JSON 字符串。请求/响应 schema、错误码、字节区间语义及 C ABI 所有权见 [API.md](API.md)。

需要个人风格时，先使用 `styleProfile` 对允许参考的笔记提取结构统计，再把结构提示交给 M3；它不返回旧笔记正文。需要展示引用时，用 `resolve` 按 `documentId + revision + chunkId` 从当前索引重新取证，避免直接信任调用方回传的 citation 文本。

## 隐私与边界

索引仅在当前进程内存存在，进程退出即丢失；目前没有持久化、认证或真实 embedding 模型。M2 不调用云服务、不生成回答。检索结果是候选证据，不等于答案正确；上层必须自行实施认证/访问控制并引用返回的证据。

## License

MIT，见 [LICENSE](LICENSE)。
