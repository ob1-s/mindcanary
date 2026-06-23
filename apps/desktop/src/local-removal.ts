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
      "This removes mindcanary's local service, browser bridge, encrypted database, and configuration from this device.",
    confirmationText:
      "Type the confirmation phrase exactly. Export or back up first if you want to keep your records.",
    excludedText:
      "Chrome extension storage and any exports or backups you created are not affected.",
    confirmationPhrase: LOCAL_REMOVAL_CONFIRMATION_PHRASE,
  };
}

export function localRemovalResultText(report: LocalRemovalReport): string {
  const parts = report.database_profile_destroyed
    ? [
        "Local database files, background service, and browser integration files have been removed",
      ]
    : [
        "No active local database was found; background service and browser integration files checked",
      ];

  parts.push("Chrome extension storage and user exports were not removed");
  return parts.join("; ");
}
