# 飞书只读四表抓取层交付报告

## 交付范围

- 仅修改 `src-tauri/src/feishu.rs`。
- 新增 `FeishuCaseManagementRecords`，统一返回 `cases / progress / stages / contacts`。
- 新增 `fetch_active_case_management_records`，抓取“在办”案件及其明确关联的进度、阶段、联系人记录。
- 未修改数据库、Tauri 命令注册或飞书同步写入逻辑。

## 实现边界

1. 从案件总表字段元数据中按字段名读取 `DuplexLink.property.table_id`，动态发现：
   - `案件进度`
   - `☑️阶段表`
   - `案件联系表`
2. 继续复用既有服务端筛选，只读取 `☑状态=在办` 的案件。
3. 仅从在办案件关联字段中的 `record_ids / link_record_ids` 收集子表记录 ID；不根据名称猜测、不全表扫描。
4. 三张子表先校验必需字段及其回链到当前案件总表的关系，再按每批最多 100 个 ID 调用只读 `records/batch_get`。
5. 字段缺失、表关系错误、分页契约异常、关联值结构异常或批量响应不完整时，整次调用失败；不返回部分成功数据。
6. 网络方法仅包括：
   - GET 字段元数据；
   - GET 在办案件记录；
   - POST `records/batch_get`（飞书官方只读批量读取接口）。
7. 生产代码未硬编码任何子表 ID，也没有飞书新增、修改、删除请求和本地数据库写入。

## 定向测试

执行：

```text
cargo test feishu::tests --lib
```

结果：15 passed，0 failed，102 filtered out。

新增测试覆盖：

- 只从 DuplexLink 元数据动态发现三张子表；
- 缺失或非双向关联字段时拒绝；
- 关联记录 ID 去重且只接受显式 ID；
- 异常关联值不猜测；
- batch_get 响应结构完整性；
- 子表必须回链当前案件总表。

## 验收结论

本子任务达到“只读、动态发现、仅在办、失败封闭、无数据库写入、无硬编码表 ID”的提交审查条件。是否接受由主控决定。
