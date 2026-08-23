const passwordChecks = [
  (value: string) => value.length >= 8,
  (value: string) => /[a-z]/u.test(value),
  (value: string) => /[A-Z]/u.test(value),
  (value: string) => /\d/u.test(value),
  (value: string) => /[^A-Za-z\d]/u.test(value),
];

export function passwordStrengthScore(value: string): number {
  return passwordChecks.filter((check) => check(value)).length;
}

export function generatePassword(length = 16): string {
  const groups = ['abcdefghijkmnopqrstuvwxyz', 'ABCDEFGHJKLMNPQRSTUVWXYZ', '23456789', '!@#$%^&*()-_=+'];
  const required = groups.map((group) => randomCharacter(group));
  const all = groups.join('');
  while (required.length < length) required.push(randomCharacter(all));
  for (let index = required.length - 1; index > 0; index -= 1) {
    const target = randomIndex(index + 1);
    const value = required[index];
    const targetValue = required[target];
    if (value === undefined || targetValue === undefined) continue;
    required[index] = targetValue;
    required[target] = value;
  }
  return required.join('');
}

function randomCharacter(characters: string): string {
  return characters[randomIndex(characters.length)] ?? characters[0] ?? '';
}

function randomIndex(maximum: number): number {
  const values = new Uint32Array(1);
  globalThis.crypto.getRandomValues(values);
  return (values[0] ?? 0) % maximum;
}
