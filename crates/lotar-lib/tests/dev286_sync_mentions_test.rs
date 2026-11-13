//! DEV-286: writes made while mention linking is paused (sync pulls) don't
//! turn IDs in the text into relationships.

use crate::common;
use lotar::api_types::TaskCreate;
use lotar::services::task_service::{MentionLinkingPaused, TaskService};

#[test]
fn paused_writes_do_not_link_mentions() {
    let fixtures = common::TestFixtures::new();
    let mut storage = fixtures.create_storage();
    let create = |storage: &mut lotar::storage::manager::Storage, title: &str, desc: &str| {
        TaskService::create(storage, TaskCreate { title: title.into(), project: Some("ENG".into()), description: Some(desc.into()), ..Default::default() })
            .unwrap()
    };
    create(&mut storage, "first", "");
    {
        let _paused = MentionLinkingPaused::new();
        let pulled = create(&mut storage, "pulled", "See upstream ENG-1 for context");
        assert!(pulled.relationships.related.is_empty(), "{:?}", pulled.relationships);
    }
    let local = create(&mut storage, "local", "Follows ENG-1");
    assert_eq!(local.relationships.related, vec!["ENG-1".to_string()]);
}
