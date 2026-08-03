import { invoke } from "@tauri-apps/api/core";

// --- Cash Session Types ---

export interface OpenSessionResponse {
  sessionId: string;
  openingAmount: string;
}

export interface ClosedSessionResponse {
  sessionId: string;
  openingAmount: string;
  expectedCash: string;
  countedCash: string;
  difference: string;
}

export interface ReconciliationView {
  sessionId: string;
  openingAmount: string;
  expectedCash: string;
  countedCash?: string;
  difference?: string;
}

// --- Sale Types ---

export type SaleCategory = "ropa" | "libreria" | "mixto";
export type PaymentMethod = "cash" | "transfer" | "debit" | "credit" | "qr";

export interface PaymentPartRequest {
  method: PaymentMethod;
  amount: string;
  recipientAccount?: string;
}

export interface RecordSaleRequest {
  totalAmount: string;
  category: SaleCategory;
  detail?: string;
  payments: PaymentPartRequest[];
}

export interface SaleRecordedResponse {
  saleId: string;
  totalAmount: string;
}

export interface CashOutRecordedResponse {
  movementId: string;
  amount: string;
}

export interface AnnulSaleRequest {
  saleId: string;
  reason?: string;
}

export interface SaleAnnulledResponse {
  saleId: string;
  status: "annulled";
}

export interface SessionPayment {
  method: PaymentMethod;
  amount: string;
  recipientAccount?: string;
}

export interface SessionOperation {
  id: string;
  occurredAt: string;
  operationType: "sale" | "cash_out";
  amount: string;
  detail?: string;
  category?: SaleCategory;
  payments: SessionPayment[];
  status?: "confirmed" | "annulled";
  annulmentReason?: string;
}

export interface PaymentTotals {
  cash: string;
  transfer: string;
  debit: string;
  credit: string;
  qr: string;
}

export interface TransferAccountTotal {
  recipientAccount: string;
  amount: string;
}

export interface SessionHistoryResponse {
  operations: SessionOperation[];
  paymentTotals: PaymentTotals;
  transferTotals: TransferAccountTotal[];
  expectedCash: string;
}

export interface ClosedSessionSummary {
  sessionId: string;
  openedAt: string;
  closedAt: string;
  openingAmount: string;
  expectedCash: string;
  countedCash: string;
  difference: string;
}

export interface ClosedSessionDetail extends ClosedSessionSummary {
  operations: SessionOperation[];
  paymentTotals: PaymentTotals;
  transferTotals: TransferAccountTotal[];
}

// --- Cash Session Commands ---

export async function openSession(
  openingAmount: string,
  idempotencyKey: string,
): Promise<OpenSessionResponse> {
  return invoke<OpenSessionResponse>("open_session", {
    openingAmount,
    idempotencyKey,
  });
}

export async function closeSession(
  countedCash: string,
  idempotencyKey: string,
): Promise<ClosedSessionResponse> {
  return invoke<ClosedSessionResponse>("close_session", {
    countedCash,
    idempotencyKey,
  });
}

export async function getReconciliation(
  countedCash?: string,
): Promise<ReconciliationView> {
  return invoke<ReconciliationView>("get_reconciliation", {
    countedCash: countedCash ?? null,
  });
}

export async function getSessionHistory(): Promise<SessionHistoryResponse> {
  return invoke<SessionHistoryResponse>("get_session_history");
}

export async function listClosedSessions(): Promise<ClosedSessionSummary[]> {
  return invoke<ClosedSessionSummary[]>("list_closed_sessions");
}

export async function getClosedSessionDetail(
  sessionId: string,
): Promise<ClosedSessionDetail> {
  return invoke<ClosedSessionDetail>("get_closed_session_detail", {
    sessionId,
  });
}

// --- Sale Commands ---

export async function recordSale(
  request: RecordSaleRequest,
  idempotencyKey: string,
): Promise<SaleRecordedResponse> {
  return invoke<SaleRecordedResponse>("record_sale", {
    request,
    idempotencyKey,
  });
}

export async function recordCashOut(
  amount: string,
  detail: string | undefined,
  idempotencyKey: string,
): Promise<CashOutRecordedResponse> {
  return invoke<CashOutRecordedResponse>("record_cash_out", {
    amount,
    detail: detail ?? null,
    idempotencyKey,
  });
}

export async function annulSale(
  request: AnnulSaleRequest,
  idempotencyKey: string,
): Promise<SaleAnnulledResponse> {
  return invoke<SaleAnnulledResponse>("annul_sale", {
    request,
    idempotencyKey,
  });
}
