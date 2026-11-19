//! 覆写模板业务测试：CRUD、重名拒绝、非法 TS/URL 不落盘、删除被
//! 订阅引用时拒绝、解绑后删除、同根并发绑定/删除不残留悬空引用。
//! 不联网：远程覆写地址只保存不下载。

use super::*;
use crate::subscriptions::Subscriptions;

const VALID_JS: &str = "function main(config: Record<string, unknown>) {\n  return config;\n}\n";

#[test]
fn empty_root_loads_an_empty_profile_list() {
    let temporary = tempfile::tempdir().unwrap();
    let profiles = Profiles::for_root(temporary.path()).unwrap();
    assert!(profiles.load().unwrap().is_empty());
    assert!(!temporary.path().join("profiles.json").exists());
}

#[test]
fn save_creates_and_reopen_restores_everything() {
    let temporary = tempfile::tempdir().unwrap();
    let profiles = Profiles::for_root(temporary.path()).unwrap();
    let saved = profiles
        .save(
            None,
            " 东京模板 ",
            VALID_JS.to_string(),
            Some("".to_string()),
        )
        .unwrap();
    // 名称 trim、空白 URL 归一化为 None、多行内容原样保留。
    assert_eq!(saved.name, "东京模板");
    assert!(saved.yaml_url.is_none());
    assert_eq!(saved.js_url, None);
    assert!(saved.yaml_override.is_empty());
    assert_eq!(saved.js_override, VALID_JS);

    // 同一数据根上的“重开”：释放业务句柄后重新打开。
    let reopened = Profiles::for_root(temporary.path()).unwrap();
    let loaded = reopened.load().unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].id, saved.id);
    assert_eq!(loaded[0].name, "东京模板");
    assert!(loaded[0].yaml_override.is_empty());
    assert_eq!(loaded[0].js_url, None);
}

#[test]
fn empty_overrides_and_urls_are_allowed() {
    let temporary = tempfile::tempdir().unwrap();
    let profiles = Profiles::for_root(temporary.path()).unwrap();
    let saved = profiles
        .save(None, "空白模板", String::new(), None)
        .unwrap();
    assert_eq!(saved.yaml_override, "");
    assert_eq!(saved.js_override, "");
    assert_eq!(saved.yaml_url, None);
    assert_eq!(saved.js_url, None);
}

#[test]
fn duplicate_name_is_rejected_except_when_editing_itself() {
    let temporary = tempfile::tempdir().unwrap();
    let profiles = Profiles::for_root(temporary.path()).unwrap();
    let first = profiles.save(None, "一号", String::new(), None).unwrap();
    profiles.save(None, "二号", String::new(), None).unwrap();

    let error = profiles
        .save(None, "一号", String::new(), None)
        .unwrap_err();
    assert!(error.to_string().contains("已存在"), "{error}");

    // 编辑自身（不改名）允许；trim 后与自身相同的名称也允许。
    profiles
        .save(Some(first.id), " 一号 ", String::new(), None)
        .unwrap();
    let loaded = profiles.load().unwrap();
    assert_eq!(loaded.len(), 2);
    let edited = loaded.iter().find(|p| p.id == first.id).unwrap();
    assert_eq!(edited.name, "一号");
    assert!(edited.yaml_override.is_empty());
}

#[test]
fn empty_and_whitespace_names_are_rejected() {
    let temporary = tempfile::tempdir().unwrap();
    let profiles = Profiles::for_root(temporary.path()).unwrap();
    for name in ["", "   ", "\t\n"] {
        assert!(
            profiles.save(None, name, String::new(), None).is_err(),
            "accepted {name:?}"
        );
    }
    assert!(profiles.load().unwrap().is_empty());
    assert!(!temporary.path().join("profiles.json").exists());
}

#[test]
fn invalid_yaml_js_and_urls_are_rejected_without_persisting() {
    let temporary = tempfile::tempdir().unwrap();
    let profiles = Profiles::for_root(temporary.path()).unwrap();
    profiles.save(None, "保留", String::new(), None).unwrap();

    let cases = [
        ("console.log(1)", None),
        ("function main(c: ) { return c; }", None),
        ("", Some("not-a-url".to_owned())),
        ("", Some("ftp://example.com/override.ts".to_owned())),
    ];
    for (ts, url) in cases {
        assert!(profiles.save(None, "坏模板", ts.to_owned(), url).is_err());
    }

    let loaded = profiles.load().unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].name, "保留");
    let text = std::fs::read_to_string(temporary.path().join("profiles.json")).unwrap();
    assert!(text.contains("保留"));
    assert!(!text.contains("坏模板"));
}

#[test]
fn edit_updates_fields_and_keeps_id() {
    let temporary = tempfile::tempdir().unwrap();
    let profiles = Profiles::for_root(temporary.path()).unwrap();
    let created = profiles.save(None, "原名", String::new(), None).unwrap();
    let edited = profiles
        .save(
            Some(created.id),
            "新名",
            VALID_JS.to_string(),
            Some("https://example.com/remote.js".to_string()),
        )
        .unwrap();
    assert_eq!(edited.id, created.id);
    assert_eq!(edited.name, "新名");
    assert_eq!(
        edited.js_url.as_deref(),
        Some("https://example.com/remote.js")
    );

    // 编辑不存在的模板报错且不动清单。
    let missing = profiles
        .save(Some(Uuid::new_v4()), "幽灵", String::new(), None)
        .unwrap_err();
    assert!(missing.to_string().contains("不存在"), "{missing}");
    assert_eq!(profiles.load().unwrap().len(), 1);
}

#[test]
fn removing_a_bound_profile_is_rejected_until_unbound() {
    let temporary = tempfile::tempdir().unwrap();
    let profiles = Profiles::for_root(temporary.path()).unwrap();
    let subscriptions = Subscriptions::for_root(temporary.path()).unwrap();
    let profile = profiles
        .save(None, "被绑模板", String::new(), None)
        .unwrap();
    let subscription = subscriptions
        .add("绑定者", "https://example.com/a.yaml", None)
        .unwrap();
    subscriptions
        .update(
            subscription.id,
            "绑定者",
            "https://example.com/a.yaml",
            None,
            Some(profile.id),
        )
        .unwrap();

    // 被引用时删除必须拒绝，引用关系不被静默解除。
    let error = profiles.remove(profile.id).unwrap_err();
    assert!(error.to_string().contains("请先取消关联"), "{error}");
    assert_eq!(profiles.load().unwrap().len(), 1);
    let reloaded = subscriptions.load().unwrap();
    assert_eq!(reloaded[0].profile_id, Some(profile.id));

    // 解除绑定后删除成功；订阅侧不残留引用。
    subscriptions
        .update(
            subscription.id,
            "绑定者",
            "https://example.com/a.yaml",
            None,
            None,
        )
        .unwrap();
    profiles.remove(profile.id).unwrap();
    assert!(profiles.load().unwrap().is_empty());
    let reloaded = subscriptions.load().unwrap();
    assert_eq!(reloaded[0].profile_id, None);
}

#[test]
fn binding_validates_that_the_profile_exists() {
    let temporary = tempfile::tempdir().unwrap();
    let subscriptions = Subscriptions::for_root(temporary.path()).unwrap();
    let subscription = subscriptions
        .add("校验绑定", "https://example.com/a.yaml", None)
        .unwrap();

    // 不存在的模板：真实错误，不落盘、不静默改绑。
    let ghost = Uuid::new_v4();
    let error = subscriptions
        .update(
            subscription.id,
            "校验绑定",
            "https://example.com/a.yaml",
            None,
            Some(ghost),
        )
        .unwrap_err();
    assert!(error.to_string().contains("覆写模板不存在"), "{error}");
    assert_eq!(subscriptions.load().unwrap()[0].profile_id, None);

    // profiles.json 损坏（共享层回退空列表）时同样拒绝绑定。
    std::fs::write(temporary.path().join("profiles.json"), "{broken").unwrap();
    let profiles = Profiles::for_root(temporary.path()).unwrap();
    let profile = profiles
        .save(None, "恢复模板", String::new(), None)
        .unwrap();
    std::fs::write(temporary.path().join("profiles.json"), "{broken").unwrap();
    let error = subscriptions
        .update(
            subscription.id,
            "校验绑定",
            "https://example.com/a.yaml",
            None,
            Some(profile.id),
        )
        .unwrap_err();
    assert!(error.to_string().contains("覆写模板不存在"), "{error}");
    assert_eq!(subscriptions.load().unwrap()[0].profile_id, None);
}

#[test]
fn new_subscription_has_no_binding_by_default() {
    let temporary = tempfile::tempdir().unwrap();
    let subscriptions = Subscriptions::for_root(temporary.path()).unwrap();
    let added = subscriptions
        .add("默认无覆写", "https://example.com/a.yaml", None)
        .unwrap();
    assert_eq!(added.profile_id, None);
}

/// 同根并发下不变量：任一订阅引用的模板必须仍存在（删除被引用模板
/// 在同一业务锁内被拒绝）。Profiles/Subscriptions 共用数据根的业务锁，
/// 绑定校验与删除引用检查不会交错出悬空引用。
#[test]
fn concurrent_bind_and_remove_never_leaves_a_dangling_reference() {
    let temporary = tempfile::tempdir().unwrap();
    let subscriptions = Subscriptions::for_root(temporary.path()).unwrap();
    let profiles = Profiles::for_root(temporary.path()).unwrap();
    let subscription = subscriptions
        .add("并发订阅", "https://example.com/c.yaml", None)
        .unwrap();
    let profile = profiles
        .save(None, "并发模板", String::new(), None)
        .unwrap();

    let mut handles = Vec::new();
    for _ in 0..4 {
        let subscriptions = subscriptions.clone();
        let bind_profiles = Profiles::for_root(temporary.path()).unwrap();
        let sid = subscription.id;
        let pid = profile.id;
        handles.push(std::thread::spawn(move || {
            for _ in 0..50 {
                let _ = subscriptions.update(
                    sid,
                    "并发订阅",
                    "https://example.com/c.yaml",
                    None,
                    Some(pid),
                );
                let _ =
                    subscriptions.update(sid, "并发订阅", "https://example.com/c.yaml", None, None);
                // 绑定路径的另一种入口：校验 + 落盘必须在同一把锁内。
                let _ = bind_profiles.load();
            }
        }));
    }
    for _ in 0..2 {
        let remove_profiles = Profiles::for_root(temporary.path()).unwrap();
        let pid = profile.id;
        handles.push(std::thread::spawn(move || {
            for _ in 0..100 {
                let _ = remove_profiles.remove(pid);
            }
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }

    let remaining = profiles.load().unwrap();
    for subscription in subscriptions.load().unwrap() {
        if let Some(bound) = subscription.profile_id {
            assert!(
                remaining.iter().any(|profile| profile.id == bound),
                "subscription {} references a removed profile {bound}",
                subscription.id
            );
        }
    }
}
