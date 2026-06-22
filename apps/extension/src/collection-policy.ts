import type {
  Metric,
  SignalCollectionSetting,
  SignalId,
} from "@mindcanary/protocol";

export function enabledSignalIds(
  settings: SignalCollectionSetting[],
): SignalId[] {
  return settings
    .filter((setting) => setting.enabled)
    .map((setting) => setting.signal);
}

export function filterEnabledMetrics(
  metrics: Metric[],
  enabledSignals: ReadonlySet<SignalId>,
): Metric[] {
  return metrics.filter((metric) => enabledSignals.has(metric.signal));
}
