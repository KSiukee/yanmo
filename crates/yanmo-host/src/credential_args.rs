//! 子宿主启动参数里的**凭据绑定**：只解析名字/域名，不碰秘密。

use yanmo_vault::CredentialSpec;

pub fn credentials_for(
    module_id: Option<&str>,
    raw: Option<&str>,
) -> Vec<CredentialSpec> {
    let (Some(module_id), Some(raw)) = (module_id, raw) else {
        return Vec::new();
    };
    let Ok(map) =
        serde_json::from_str::<std::collections::HashMap<String, Vec<CredentialSpec>>>(raw)
    else {
        return Vec::new();
    };
    map.get(module_id).cloned().unwrap_or_default()
}
