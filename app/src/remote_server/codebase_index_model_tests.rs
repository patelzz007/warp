use super::*;

fn host() -> HostId {
    HostId::new("host".to_string())
}
fn host_with_name(name: &str) -> HostId {
    HostId::new(name.to_string())
}

fn remote_path(repo_path: &str) -> RemotePath {
    remote_path_from_repo_path(&host(), repo_path).unwrap()
}

fn remote_path_for_host(host: &HostId, repo_path: &str) -> RemotePath {
    remote_path_from_repo_path(host, repo_path).unwrap()
}

fn ready_status(repo_path: &str) -> RemoteCodebaseIndexStatus {
    RemoteCodebaseIndexStatus {
        repo_path: repo_path.to_string(),
        state: RemoteCodebaseIndexState::Ready,
        last_updated_epoch_millis: Some(1),
        progress_completed: None,
        progress_total: None,
        failure_message: None,
        root_hash: Some(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
        ),
    }
}
fn status_with_path(repo_path: &str) -> RemoteCodebaseIndexStatusWithPath {
    RemoteCodebaseIndexStatusWithPath {
        remote_path: remote_path(repo_path),
        status: ready_status(repo_path),
    }
}

#[test]
fn snapshot_replaces_statuses_for_host() {
    let mut model = RemoteCodebaseIndexModel::default();
    let host = host();
    model.apply_status_update(remote_path("/old"), ready_status("/old"));
    model.apply_statuses_snapshot(&host, &[status_with_path("/new")]);

    assert!(model.status_for_repo(&remote_path("/old")).is_none());
    assert!(model.status_for_repo(&remote_path("/new")).is_some());
}

#[test]
fn entries_for_settings_are_sorted_by_host_then_path() {
    let mut model = RemoteCodebaseIndexModel::default();
    let host_b = host_with_name("host-b");
    let host_a = host_with_name("host-a");
    model.apply_status_update(
        remote_path_for_host(&host_b, "/z-repo"),
        ready_status("/z-repo"),
    );
    model.apply_status_update(
        remote_path_for_host(&host_a, "/b-repo"),
        ready_status("/b-repo"),
    );
    model.apply_status_update(
        remote_path_for_host(&host_a, "/a-repo"),
        ready_status("/a-repo"),
    );

    let entries = model.entries_for_settings();
    let labels_and_paths = entries
        .iter()
        .map(|entry| (entry.host_label.as_str(), entry.remote_path.path.as_str()))
        .collect::<Vec<_>>();

    assert_eq!(
        labels_and_paths,
        vec![
            ("host-a", "/a-repo"),
            ("host-a", "/b-repo"),
            ("host-b", "/z-repo")
        ]
    );
}

#[test]
fn entries_for_settings_use_host_label_when_available() {
    let mut model = RemoteCodebaseIndexModel::default();
    let host = host();
    model
        .host_labels
        .insert(host.clone(), "kevinyang@ssh-testing".to_string());
    model.apply_status_update(remote_path("/repo"), ready_status("/repo"));

    let entries = model.entries_for_settings();
    assert_eq!(entries[0].host_label, "kevinyang@ssh-testing");
}

#[test]
fn entries_for_settings_fall_back_to_host_id_without_label() {
    let mut model = RemoteCodebaseIndexModel::default();
    let host = host();
    model.apply_status_update(remote_path("/repo"), ready_status("/repo"));

    let entries = model.entries_for_settings();

    assert_eq!(entries[0].host_label, host.to_string());
}

#[test]
fn host_disconnect_marks_settings_entries_unavailable_without_removing_them() {
    let mut model = RemoteCodebaseIndexModel::default();
    let host = host();
    model.apply_status_update(remote_path("/repo"), ready_status("/repo"));
    model.record_navigated_directory(&remote_path("/repo"));

    model.mark_host_unavailable(&host);

    let status = model.status_for_repo(&remote_path("/repo")).unwrap();
    assert_eq!(status.state, RemoteCodebaseIndexState::Unavailable);
    assert_eq!(
        status.failure_message.as_deref(),
        Some("The remote host is currently disconnected.")
    );
    assert_eq!(model.entries_for_settings().len(), 1);
    assert!(matches!(
        model.availability_for_remote(&host, Some("/repo"), None),
        RemoteCodebaseSearchAvailability::Unavailable { .. }
    ));
}

#[test]
fn availability_uses_active_navigated_repo() {
    let mut model = RemoteCodebaseIndexModel::default();
    let host = host();
    model.record_navigated_directory(&remote_path("/repo"));
    model.apply_status_update(remote_path("/repo"), ready_status("/repo"));

    let availability = model.availability_for_remote(&host, Some("/repo/src"), None);

    assert!(availability.is_ready());
    assert_eq!(availability.repo_path(), Some("/repo"));
}

#[test]
fn availability_uses_active_navigated_non_git_directory() {
    let mut model = RemoteCodebaseIndexModel::default();
    let host = host();
    model.record_navigated_directory(&remote_path("/directory"));
    model.apply_status_update(remote_path("/directory"), ready_status("/directory"));

    let availability = model.availability_for_remote(&host, Some("/repo/src"), None);

    assert!(availability.is_ready());
    assert_eq!(availability.repo_path(), Some("/directory"));
}

#[test]
fn availability_falls_back_to_longest_status_prefix() {
    let mut model = RemoteCodebaseIndexModel::default();
    let host = host();
    model.apply_status_update(remote_path("/repo"), ready_status("/repo"));
    model.apply_status_update(remote_path("/repo/nested"), ready_status("/repo/nested"));

    let availability = model.availability_for_remote(&host, Some("/repo/nested/src"), None);

    assert!(availability.is_ready());
    assert_eq!(availability.repo_path(), Some("/repo/nested"));
}

#[test]
fn indexing_state_is_not_ready() {
    let mut status = ready_status("/repo");
    status.state = RemoteCodebaseIndexState::Indexing;

    let availability = search_availability_for_status(&status, remote_path("/repo"));

    assert!(matches!(
        availability,
        RemoteCodebaseSearchAvailability::Indexing { .. }
    ));
}

#[test]
fn missing_root_hash_is_unavailable() {
    let mut status = ready_status("/repo");
    status.root_hash = None;

    let availability = search_availability_for_status(&status, remote_path("/repo"));

    assert!(matches!(
        availability,
        RemoteCodebaseSearchAvailability::Unavailable { .. }
    ));
}

#[test]
fn remote_auto_indexing_requires_feature_codebase_context_and_auto_indexing() {
    {
        let _remote_flag = FeatureFlag::RemoteCodebaseIndexing.override_enabled(true);
        let _flag = FeatureFlag::FullSourceCodeEmbedding.override_enabled(false);
        assert!(!remote_auto_indexing_enabled(true, true));
    }
    {
        let _remote_flag = FeatureFlag::RemoteCodebaseIndexing.override_enabled(true);
        let _flag = FeatureFlag::FullSourceCodeEmbedding.override_enabled(true);
        assert!(remote_auto_indexing_enabled(true, true));
        assert!(!remote_auto_indexing_enabled(false, true));
        assert!(!remote_auto_indexing_enabled(true, false));
        assert!(!remote_auto_indexing_enabled(false, false));
    }
    {
        let _remote_flag = FeatureFlag::RemoteCodebaseIndexing.override_enabled(false);
        let _flag = FeatureFlag::FullSourceCodeEmbedding.override_enabled(true);
        assert!(!remote_auto_indexing_enabled(true, true));
    }
}
