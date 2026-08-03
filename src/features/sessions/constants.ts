import type { PaymentMethod, SaleCategory } from "../../shared/tauri/commands";

export const SALE_CATEGORIES: { value: SaleCategory; label: string }[] = [
  { value: "ropa", label: "Ropa" },
  { value: "libreria", label: "Librería" },
  { value: "mixto", label: "Mixto" },
];

export const PAYMENT_METHODS: { value: PaymentMethod; label: string }[] = [
  { value: "cash", label: "Efectivo" },
  { value: "transfer", label: "Transferencia" },
  { value: "debit", label: "Débito" },
  { value: "credit", label: "Crédito" },
  { value: "qr", label: "QR" },
];

export const RECIPIENT_ACCOUNTS = ["Juan", "Carlos"];

export function paymentMethodLabel(method: PaymentMethod): string {
  return (
    PAYMENT_METHODS.find((paymentMethod) => paymentMethod.value === method)
      ?.label ?? method
  );
}

export function saleCategoryLabel(category: SaleCategory): string {
  return (
    SALE_CATEGORIES.find((saleCategory) => saleCategory.value === category)
      ?.label ?? category
  );
}
