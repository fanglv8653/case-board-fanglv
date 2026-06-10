//! Dev probe:从环境变量 CHINESELAW_API_KEY / YUANDIAN_API_KEY 读 key,
//! 验证元典 API client 能不能跑通(查一个公开企业作为连通性测试主体)。
//!
//! 用法:
//!   CHINESELAW_API_KEY=sk_xxxxxx cargo run --example yuandian_probe [公司名]
//!   不传公司名时默认查"阿里巴巴"。

use caseboard_lib::yuandian::{
    enterprise_aggregation_summary, enterprise_executed_person, enterprise_search, EntityId,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = std::env::var("CHINESELAW_API_KEY")
        .or_else(|_| std::env::var("YUANDIAN_API_KEY"))
        .map_err(|_| "未设置 CHINESELAW_API_KEY 或 YUANDIAN_API_KEY 环境变量")?;

    let query = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "阿里巴巴".to_string());
    println!("=== 1. enterprise_search(\"{}\") ===", query);
    let r = enterprise_search(&api_key, &query).await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&r).unwrap_or_else(|_| "<not pretty>".into())
    );

    // 从结果取第一个企业的 id 做后续查询
    let first_id = r
        .get("data")
        .and_then(|d| d.as_array())
        .and_then(|arr| arr.first())
        .and_then(|item| item.get("id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    if let Some(id) = first_id {
        println!("\n=== 2. enterprise_aggregation_summary(id={}) ===", id);
        let r2 = enterprise_aggregation_summary(&api_key, &EntityId::Id(id.clone())).await?;
        // 太长只打前 800 字符
        let s = serde_json::to_string(&r2)?;
        println!("{}…", &s.chars().take(800).collect::<String>());

        println!("\n=== 3. enterprise_executed_person(id={}) ===", id);
        let r3 = enterprise_executed_person(&api_key, &EntityId::Id(id), 1).await?;
        let s3 = serde_json::to_string(&r3)?;
        println!("{}", s3.chars().take(500).collect::<String>());
    }

    println!("\n✅ 元典 API 连通性测试完成");
    Ok(())
}
