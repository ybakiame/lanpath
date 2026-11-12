//! Headless 测试（`nsh_testing::Headless`，NSH 0.5.0）：覆写模板页真实列表、
//! 多行覆写编辑、持久化、删除确认与订阅绑定/解绑/删除拒绝。
//! 每个场景使用独立临时数据根；本切片不为远程覆写地址联网。

mod common;

use common::{data_root, start_app, texts};

/// 启动一次应用并等订阅页渲染完成：数据根由此初始化（写入归属标记），
/// 之后才能在盘上预置种子业务文件（直写文件绕过准入会拒绝）。
fn initialize_root(entries: std::collections::BTreeMap<String, String>) {
    let mut first = start_app(entries).0;
    first.wait_for("尚无订阅。").unwrap();
    drop(first);
}

fn profile_id_of(dir: &tempfile::TempDir, name: &str) -> uuid::Uuid {
    let profiles = lanpath_client::ProfileStoreV2::new(dir.path().to_path_buf())
        .load()
        .unwrap();
    profiles
        .iter()
        .find(|profile| profile.name == name)
        .unwrap_or_else(|| panic!("profile {name} not found"))
        .id
}

/// 侧栏导航到覆写页：列表为空提示与新建入口真实展示。
#[test]
fn navigating_to_the_profiles_page_shows_an_empty_list() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    app.wait_for("尚无订阅。").unwrap();
    app.press("覆写").unwrap();
    app.wait_for("尚无覆写模板。").unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"共 0 个覆写模板"), "{seen:?}");
    assert!(seen.contains(&"新建覆写模板"), "{seen:?}");
    assert!(
        seen.iter()
            .any(|text| text.contains("远程覆写地址仅在配置合成时解析")),
        "{seen:?}"
    );
}

/// 新建模板：真实多行输入保留换行，保存后列表更新并持久化；
/// 重开应用后编辑框回读内容与磁盘一致。
#[test]
fn creating_a_profile_with_multiline_overrides_persists_and_reopens() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries.clone()).0;
    app.wait_for("尚无订阅。").unwrap();
    app.press("覆写").unwrap();
    app.wait_for("尚无覆写模板。").unwrap();

    app.press("新建覆写模板").unwrap();
    app.wait_for("新建覆写模板").unwrap();
    app.type_text("id:profile-name", " 东京模板 ").unwrap();
    app.type_text("id:profile-yaml-url", " https://example.com/y.yaml ")
        .unwrap();
    const YAML: &str = "dns:\n  enabled: true";
    const JS: &str = "function main(config) {\n  return config;\n}";
    app.type_text("id:profile-yaml", YAML).unwrap();
    app.type_text("id:profile-js", JS).unwrap();
    app.press("保存").unwrap();
    app.wait_for("覆写模板已校验并保存").unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"共 1 个覆写模板"), "{seen:?}");
    assert!(seen.contains(&"东京模板"), "{seen:?}");
    // 行内摘要是一整个文本元素：逐项子串断言。
    let summary = |needle: &str| seen.iter().any(|text| text.contains(needle));
    for expected in [
        "本地 YAML 覆写：已填写",
        "本地 JS 覆写：已填写",
        "远程 YAML：已配置",
        "远程 JS：未配置",
    ] {
        assert!(summary(expected), "missing {expected}: {seen:?}");
    }
    // 空白远程地址未伪造“已配置”。
    drop(app);

    // 同一数据根重开：模板从磁盘恢复；编辑回读多行内容原样（换行保留）。
    let mut reopened = start_app(entries).0;
    reopened.wait_for("尚无订阅。").unwrap();
    reopened.press("覆写").unwrap();
    reopened.wait_for("东京模板").unwrap();
    reopened
        .press("id:profile-row-东京模板 >> text:编辑")
        .unwrap();
    reopened.wait_for("编辑覆写模板“东京模板”").unwrap();
    assert_eq!(
        reopened.screen().input_text("id:profile-name").unwrap(),
        Some("东京模板")
    );
    assert_eq!(
        reopened.screen().input_text("id:profile-yaml").unwrap(),
        Some(YAML)
    );
    assert_eq!(
        reopened.screen().input_text("id:profile-js").unwrap(),
        Some(JS)
    );
    assert_eq!(
        reopened.screen().input_text("id:profile-yaml-url").unwrap(),
        Some("https://example.com/y.yaml")
    );
}

/// 编辑草稿：修改后取消需确认；继续编辑保留草稿，确认放弃后列表不变。
#[test]
fn cancelling_a_profile_edit_with_changes_asks_first() {
    let (dir, entries) = data_root();
    // 先让应用初始化数据根（写入归属标记），再落盘种子数据。
    initialize_root(entries.clone());
    lanpath_client::ProfileStoreV2::new(dir.path().to_path_buf())
        .save(&[lanpath_client::Profile {
            id: uuid::Uuid::new_v4(),
            name: "草稿模板".to_owned(),
            yaml_override: String::new(),
            js_override: String::new(),
            yaml_url: None,
            js_url: None,
        }])
        .unwrap();
    let mut app = start_app(entries).0;
    app.wait_for("尚无订阅。").unwrap();
    app.press("覆写").unwrap();
    app.wait_for("草稿模板").unwrap();

    app.press("id:profile-row-草稿模板 >> text:编辑").unwrap();
    app.wait_for("编辑覆写模板“草稿模板”").unwrap();
    app.clear("id:profile-name").unwrap();
    app.type_text("id:profile-name", "不应保存").unwrap();
    app.press("取消").unwrap();
    app.wait_for("放弃未保存的覆写修改？").unwrap();
    // 继续编辑：草稿还在。
    app.press("继续编辑").unwrap();
    assert_eq!(
        app.screen().input_text("id:profile-name").unwrap(),
        Some("不应保存")
    );
    // 再次取消并确认放弃：列表仍是原名。
    app.press("取消").unwrap();
    app.wait_for("放弃未保存的覆写修改？").unwrap();
    app.press("放弃修改").unwrap();
    app.wait_for("共 1 个覆写模板").unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"草稿模板"), "{seen:?}");
    assert!(!seen.contains(&"不应保存"), "{seen:?}");
}

/// 非法输入拒绝并显示真实错误：重名、非法 YAML、非法远程 URL；
/// 已有模板与磁盘内容不被覆盖。
#[test]
fn invalid_profile_input_is_rejected_without_overwriting_existing_data() {
    let (dir, entries) = data_root();
    initialize_root(entries.clone());
    lanpath_client::ProfileStoreV2::new(dir.path().to_path_buf())
        .save(&[lanpath_client::Profile {
            id: uuid::Uuid::new_v4(),
            name: "保留模板".to_owned(),
            yaml_override: String::new(),
            js_override: String::new(),
            yaml_url: None,
            js_url: None,
        }])
        .unwrap();
    let mut app = start_app(entries).0;
    app.wait_for("尚无订阅。").unwrap();
    app.press("覆写").unwrap();
    app.wait_for("保留模板").unwrap();

    // 重名：拒绝，草稿保留。
    app.press("新建覆写模板").unwrap();
    app.wait_for("新建覆写模板").unwrap();
    app.type_text("id:profile-name", "保留模板").unwrap();
    app.press("保存").unwrap();
    app.wait_for("text:/覆写模板名称「保留模板」已存在/")
        .unwrap();
    assert_eq!(
        app.screen().input_text("id:profile-name").unwrap(),
        Some("保留模板")
    );

    // 非法 YAML（非 mapping）：拒绝，不落盘。
    app.clear("id:profile-name").unwrap();
    app.type_text("id:profile-name", "坏模板").unwrap();
    app.type_text("id:profile-yaml", "- a\n- b").unwrap();
    app.press("保存").unwrap();
    app.wait_for("text:/YAML override must be a mapping/")
        .unwrap();

    // 非法 JS（缺 main）：拒绝。
    app.clear("id:profile-yaml").unwrap();
    app.type_text("id:profile-js", "console.log(1)").unwrap();
    app.press("保存").unwrap();
    app.wait_for("text:/JS override must define a main function/")
        .unwrap();

    // 非法远程 URL（非 http/https）：拒绝。
    app.clear("id:profile-js").unwrap();
    app.type_text("id:profile-yaml-url", "ftp://example.com/y.yaml")
        .unwrap();
    app.press("保存").unwrap();
    app.wait_for("text:/Remote override URL must start with http/")
        .unwrap();
    app.press("取消").unwrap();

    let seen = texts(&app);
    assert!(seen.contains(&"共 1 个覆写模板"), "{seen:?}");
    assert!(seen.contains(&"保留模板"), "{seen:?}");
    assert!(!seen.contains(&"坏模板"), "{seen:?}");
    let text = std::fs::read_to_string(dir.path().join("profiles.json")).unwrap();
    assert!(text.contains("保留模板"));
    assert!(!text.contains("坏模板"));
}

/// 完整绑定流程：新建订阅与模板 → 编辑订阅绑定模板（单选列表）→ 行内
/// 显示绑定名 → 模板页删除被拒绝 → 解绑后删除成功。全程持久化校验。
#[test]
fn binding_delete_refusal_unbind_and_profile_delete() {
    let (dir, entries) = data_root();
    let mut app = start_app(entries.clone()).0;
    app.wait_for("尚无订阅。").unwrap();

    // 订阅页新增订阅（无模板可绑，新增仍 None）。
    app.type_text("id:subscription-name", "绑定订阅").unwrap();
    app.type_text("id:subscription-url", "https://example.com/sub.yaml")
        .unwrap();
    app.press("新增订阅").unwrap();
    app.wait_for("共 1 个订阅").unwrap();
    let seen = texts(&app);
    assert!(!seen.iter().any(|text| text.contains("覆写：")), "{seen:?}");

    // 覆写页新建两个模板：单选列表须真实区分多个模板。
    app.press("覆写").unwrap();
    app.wait_for("尚无覆写模板。").unwrap();
    for name in ["甲模板", "乙模板"] {
        app.press("新建覆写模板").unwrap();
        app.wait_for("新建覆写模板").unwrap();
        app.type_text("id:profile-name", name).unwrap();
        app.press("保存").unwrap();
        app.wait_for("覆写模板已校验并保存").unwrap();
    }
    let template_a = profile_id_of(&dir, "甲模板");
    let template_b = profile_id_of(&dir, "乙模板");

    // 订阅编辑弹窗：选择第二个模板并保存。
    app.press("订阅").unwrap();
    app.wait_for("共 1 个订阅").unwrap();
    app.press("id:subscription-row-绑定订阅 >> text:编辑")
        .unwrap();
    app.wait_for("覆写模板（配置合成时应用）").unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"无覆写"), "{seen:?}");
    assert!(seen.contains(&"甲模板"), "{seen:?}");
    assert!(seen.contains(&"乙模板"), "{seen:?}");
    app.press(&format!("id:bind-profile-{template_b}")).unwrap();
    app.press("保存修改").unwrap();
    app.wait_for("已保存修改").unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"覆写：乙模板"), "{seen:?}");
    drop(app);

    // 重开后绑定仍在（持久化）。
    let mut reopened = start_app(entries.clone()).0;
    reopened.wait_for("覆写：乙模板").unwrap();

    // 模板页删除被引用模板：拒绝并显示真实错误。
    reopened.press("覆写").unwrap();
    reopened.wait_for("乙模板").unwrap();
    reopened
        .press("id:profile-row-乙模板 >> text:删除")
        .unwrap();
    reopened.wait_for("删除覆写模板“乙模板”？").unwrap();
    reopened.press("确认删除").unwrap();
    reopened
        .wait_for("text:/删除覆写模板失败:client error: 仍有订阅关联此覆写模板，请先取消关联/")
        .unwrap();
    let seen = texts(&reopened);
    assert!(seen.contains(&"共 2 个覆写模板"), "{seen:?}");

    // 订阅页解绑（选“无覆写”），行内绑定消失。
    reopened.press("订阅").unwrap();
    reopened.wait_for("共 1 个订阅").unwrap();
    reopened
        .press("id:subscription-row-绑定订阅 >> text:编辑")
        .unwrap();
    reopened.wait_for("覆写模板（配置合成时应用）").unwrap();
    reopened.press("id:bind-profile-none").unwrap();
    reopened.press("保存修改").unwrap();
    reopened.wait_for("已保存修改").unwrap();
    let seen = texts(&reopened);
    assert!(!seen.iter().any(|text| text.contains("覆写：")), "{seen:?}");
    drop(reopened);

    // 解绑持久化后，删除成功且列表/磁盘清空该模板。
    let mut third = start_app(entries).0;
    third.wait_for("共 1 个订阅").unwrap();
    third.press("覆写").unwrap();
    third.wait_for("共 2 个覆写模板").unwrap();
    third.press("id:profile-row-乙模板 >> text:删除").unwrap();
    third.wait_for("删除覆写模板“乙模板”？").unwrap();
    third.press("确认删除").unwrap();
    third.wait_for("覆写模板已删除").unwrap();
    let seen = texts(&third);
    assert!(seen.contains(&"共 1 个覆写模板"), "{seen:?}");
    let profiles = lanpath_client::ProfileStoreV2::new(dir.path().to_path_buf())
        .load()
        .unwrap();
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].id, template_a);
}

/// 引用失效真实显示：种子数据里订阅绑定的模板在磁盘上被外部删除，
/// 行内明确提示“引用失效”，不静默改绑、不伪造模板名。
#[test]
fn a_subscription_with_an_externally_removed_profile_shows_a_real_error() {
    let (dir, entries) = data_root();
    initialize_root(entries.clone());
    let profile_id = uuid::Uuid::new_v4();
    lanpath_client::ProfileStoreV2::new(dir.path().to_path_buf())
        .save(&[lanpath_client::Profile {
            id: profile_id,
            name: "将失踪".to_owned(),
            yaml_override: String::new(),
            js_override: String::new(),
            yaml_url: None,
            js_url: None,
        }])
        .unwrap();
    let store = lanpath_client::SubscriptionStore::new(dir.path().to_path_buf());
    store
        .add("失效订阅", "https://example.com/lost.yaml", true, None)
        .unwrap();
    let added = store
        .load()
        .unwrap()
        .into_iter()
        .find(|subscription| subscription.name == "失效订阅")
        .unwrap();
    store.set_profile_id(added.id, Some(profile_id)).unwrap();
    // 外部删除模板文件（绕过业务锁的非法状态）：共享层加载回退空列表。
    std::fs::remove_file(dir.path().join("profiles.json")).unwrap();

    let mut app = start_app(entries).0;
    app.wait_for("失效订阅").unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"覆写模板引用失效，请重新绑定"), "{seen:?}");
    assert!(!seen.contains(&"覆写：将失踪"), "{seen:?}");
}
