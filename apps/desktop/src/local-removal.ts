export const LOCAL_REMOVAL_CONFIRMATION_PHRASE = "DELETE LOCAL MINDCANARY DATA";

export interface LocalRemovalModel {
  title: string;
  summaryText: string;
  confirmationText: string;
  excludedText: string;
  confirmationPhrase: string;
}

export interface LocalRemovalReport {
  user_service_removed: boolean;
  native_host_manifests_removed: string[];
  database_profile_destroyed: boolean;
  package_marker_removed: boolean;
  runtime_socket_dir_removed: boolean;
  browser_extension_storage_removed: boolean;
  user_exports_removed: boolean;
}

export function localRemovalModel(): LocalRemovalModel {
  return {
    title: "Remove app-owned local data",
    summaryText:
      "This removes MindCanary's user service, native-host manifests, encrypted database profile, database key, package setup marker, and runtime socket directory.",
    confirmationText:
      "Type the confirmation phrase exactly. Export first if you want a readable copy of your records.",
    excludedText:
      "Chrome extension storage and user-created exports or backups are controlled outside this action.",
    confirmationPhrase: LOCAL_REMOVAL_CONFIRMATION_PHRASE,
  };
}

export function localRemovalResultText(report: LocalRemovalReport): string {
  const removedManifests = report.native_host_manifests_removed.join(", ");
  const manifestText =
    removedManifests.length === 0
      ? "native-host manifests checked"
      : `native-host manifests removed for ${removedManifests}`;
  return [
    report.user_service_removed
      ? "user service removed"
      : "user service checked",
    manifestText,
    report.database_profile_destroyed
      ? "database profile destroyed"
      : "database profile checked",
    report.package_marker_removed
      ? "package marker removed"
      : "package marker not present",
    report.runtime_socket_dir_removed
      ? "runtime socket directory removed"
      : "runtime socket directory not present",
    "Chrome extension storage and user exports were not removed",
  ].join("; ");
}
