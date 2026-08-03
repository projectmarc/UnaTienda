export function generateIdempotencyKey(): string {
  return crypto.randomUUID();
}

export function formatMoney(value: string): string {
  const negative = value.startsWith("-");
  const unsigned = negative ? value.slice(1) : value;
  const [whole, decimals = "00"] = unsigned.split(".");
  const grouped = whole.replace(/\B(?=(\d{3})+(?!\d))/g, ".");

  return `${negative ? "-" : ""}$ ${grouped},${decimals.padEnd(2, "0")}`;
}
