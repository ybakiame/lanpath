use super::*;

struct TestDir(std::path::PathBuf);

impl TestDir {
    fn new() -> Self {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "lanpath-client-ui-subscription-test-{}-{}",
            std::process::id(),
            n
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn fetch_metadata_does_not_revert_dashboard_binding_or_other_edits() {
    let dir = TestDir::new();
    let store = SubscriptionStore::new(dir.path().to_path_buf());
    let mut fetched = store
        .add("before", "https://example.com/a", true, None)
        .unwrap();
    let profile = Uuid::new_v4();
    store.set_profile_id(fetched.id, Some(profile)).unwrap();
    store
        .update(fetched.id, "renamed", &fetched.url, None)
        .unwrap();
    store.set_enabled(fetched.id, false).unwrap();
    fetched.node_count = 3;
    write_subscription(&store, &fetched).unwrap();
    let saved = store.load().unwrap().remove(0);
    assert_eq!(saved.profile_id, Some(profile));
    assert_eq!(saved.name, "renamed");
    assert!(!saved.enabled);
    assert_eq!(saved.node_count, 3);
    store
        .update(fetched.id, "changed", "https://example.com/b", None)
        .unwrap();
    write_subscription(&store, &fetched).unwrap();
    assert_eq!(store.load().unwrap()[0].node_count, 0);
}

#[test]
fn parse_profile_ref_maps_empty_or_none_to_none_and_parses_uuid() {
    assert_eq!(parse_profile_ref(&None).unwrap(), None);
    assert_eq!(parse_profile_ref(&Some(String::new())).unwrap(), None);
    assert_eq!(parse_profile_ref(&Some("  ".to_string())).unwrap(), None);
    let id = Uuid::new_v4();
    assert_eq!(parse_profile_ref(&Some(id.to_string())).unwrap(), Some(id));
    assert!(parse_profile_ref(&Some("not-a-uuid".to_string())).is_err());
}

#[test]
fn subscription_view_exposes_profile_id() {
    let sub = Subscription {
        id: Uuid::new_v4(),
        name: "sub".to_string(),
        url: "https://example.com/sub".to_string(),
        enabled: true,
        userinfo: None,
        node_count: 0,
        error: None,
        user_agent: None,
        format: None,
        profile_id: Some(Uuid::new_v4()),
    };
    let view = SubscriptionView::from_sub(&sub);
    assert_eq!(view.profile_id, sub.profile_id.map(|v| v.to_string()));

    let mut sub = sub;
    sub.profile_id = None;
    let view = SubscriptionView::from_sub(&sub);
    assert_eq!(view.profile_id, None);
}

#[test]
fn set_active_subscription_validates_and_persists_selection() {
    let dir = TestDir::new();
    let cfg = ClientConfig::new(
        dir.path().to_path_buf(),
        String::new(),
        String::new(),
        PathBuf::new(),
    );
    cfg.save().unwrap();

    let err =
        set_active_subscription_impl(dir.path(), Some(Uuid::new_v4().to_string())).unwrap_err();
    assert!(err.contains("不存在"), "{err}");

    let store = SubscriptionStore::new(dir.path().to_path_buf());
    let off = store
        .add("off", "https://example.com/sub", false, None)
        .unwrap();
    let err = set_active_subscription_impl(dir.path(), Some(off.id.to_string())).unwrap_err();
    assert!(err.contains("已停用"), "{err}");

    let on = store
        .add("on", "https://example.com/sub2", true, None)
        .unwrap();
    set_active_subscription_impl(dir.path(), Some(on.id.to_string())).unwrap();
    let saved = ClientConfig::load(dir.path()).unwrap();
    assert_eq!(saved.active_subscription_id, Some(on.id));

    set_active_subscription_impl(dir.path(), None).unwrap();
    let saved = ClientConfig::load(dir.path()).unwrap();
    assert_eq!(saved.active_subscription_id, None);
}

#[test]
fn disabling_selected_subscription_clears_active_selection() {
    let dir = TestDir::new();
    let store = SubscriptionStore::new(dir.path().to_path_buf());
    let sub = store
        .add("sub", "https://example.com/sub", true, None)
        .unwrap();
    let mut cfg = ClientConfig::new(
        dir.path().to_path_buf(),
        String::new(),
        String::new(),
        PathBuf::new(),
    );
    cfg.active_subscription_id = Some(sub.id);
    cfg.save().unwrap();

    set_subscription_enabled_impl(dir.path(), sub.id, false).unwrap();
    let saved = ClientConfig::load(dir.path()).unwrap();
    assert_eq!(saved.active_subscription_id, None);

    set_subscription_enabled_impl(dir.path(), sub.id, true).unwrap();
    let saved = ClientConfig::load(dir.path()).unwrap();
    assert_eq!(saved.active_subscription_id, None);

    let other = store
        .add("other", "https://example.com/other", false, None)
        .unwrap();
    let mut cfg = ClientConfig::load(dir.path()).unwrap();
    cfg.active_subscription_id = Some(sub.id);
    cfg.save().unwrap();
    set_subscription_enabled_impl(dir.path(), other.id, false).unwrap();
    let saved = ClientConfig::load(dir.path()).unwrap();
    assert_eq!(saved.active_subscription_id, Some(sub.id));
}
