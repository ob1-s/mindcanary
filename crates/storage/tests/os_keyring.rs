use mindcanary_storage::{DatabaseKey, DatabaseKeyProvider, OsKeyringKeyProvider};

#[test]
#[ignore = "uses the current desktop OS keyring"]
fn os_keyring_round_trip() {
    let provider = OsKeyringKeyProvider;
    provider.delete().unwrap();

    let key = DatabaseKey::generate().unwrap();
    provider.store(&key).unwrap();
    let loaded = provider.load().unwrap().unwrap();
    assert_eq!(loaded.as_bytes(), key.as_bytes());

    provider.delete().unwrap();
    assert!(provider.load().unwrap().is_none());
}
