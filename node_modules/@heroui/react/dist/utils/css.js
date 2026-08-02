const parseCSSTime = value => {
  if (!value) return undefined;
  const trimmed = value.trim();
  const val = parseFloat(trimmed);
  if (Number.isNaN(val)) return undefined;
  if (trimmed.endsWith("ms")) return val;
  if (trimmed.endsWith("s")) return val * 1000;
  return val;
};

export { parseCSSTime };
