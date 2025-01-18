export function remove(value) {
  value = value || '';
  if (!value.startsWith('---')) return value;
  // trim the front matter
  return value.split('---').slice(2).join('');
}
