// Model IDs remain native to each runtime. Labels are presentation only.
export const MODEL_LABEL = "pitwall.model";
export const DEFAULT_MODEL = "default";
export function modelSelection(value) {
  if (value == null || value === DEFAULT_MODEL) return null;
  if (
    typeof value !== "string" ||
    !value.trim() ||
    value.length > 256 ||
    /[\x00-\x20\x7f]/.test(value)
  )
    throw new Error(
      "Invalid model selection. Choose a model advertised by the agent.",
    );
  return value;
}
export function modelLabel(value) {
  return modelSelection(value) ?? DEFAULT_MODEL;
}
export function choices(values) {
  const seen = new Set();
  return values
    .filter((m) => {
      if (!m || typeof m.id !== "string" || !m.id || seen.has(m.id))
        return false;
      try {
        modelSelection(m.id);
      } catch {
        return false;
      }
      seen.add(m.id);
      return true;
    })
    .map((m) => ({
      ...m,
      label: String(m.label || m.id).replace(/[\x00-\x1f\x7f]/g, ""),
      description: m.description?.replace(/[\x00-\x1f\x7f]/g, " "),
    }));
}
export function acpModelState(setup) {
  const option = setup.configOptions?.find(
    (o) => o.type === "select" && (o.category === "model" || o.id === "model"),
  );
  const flatten = (items) =>
    items.flatMap((o) =>
      Array.isArray(o.options)
        ? flatten(o.options)
        : [{ id: o.value, label: o.name }],
    );
  return option
    ? {
        configId: option.id,
        current: option.currentValue,
        models: choices(flatten(option.options || [])),
      }
    : {
        configId: null,
        current: setup.models?.currentModelId ?? null,
        models: choices(
          (setup.models?.availableModels || []).map((m) => ({
            id: m.modelId,
            label: m.name,
            description: m.description,
          })),
        ),
      };
}
