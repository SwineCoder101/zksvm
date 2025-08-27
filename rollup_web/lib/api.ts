export interface RollupTransaction {
  sender?: string;
  sol_transaction?: string; // Base64 serialized Solana transaction
  error?: string;
}

export interface TransactionWithHash {
  hash: string;
  transaction: string; // Base64 serialized Solana transaction
}

export interface RollupTransactionsList {
  sender?: string;
  transactions: TransactionWithHash[];
  page: number;
  per_page: number;
  total?: number;
  has_more: boolean;
  error?: string;
}

const ROLLUP_BASE_URL =
  process.env.NEXT_PUBLIC_ROLLUP_URL || "http://127.0.0.1:8080";

export async function healthCheck(): Promise<{ [key: string]: string }> {
  const response = await fetch(`${ROLLUP_BASE_URL}/`);
  if (!response.ok) {
    throw new Error("Health check failed");
  }
  return response.json();
}

export async function submitTransaction(
  senderName: string | null,
  transaction: string
): Promise<{ [key: string]: string }> {
  const rollupTx: RollupTransaction = {
    sender: senderName || undefined,
    sol_transaction: transaction,
    error: undefined,
  };

  const response = await fetch(`${ROLLUP_BASE_URL}/submit_transaction`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify(rollupTx),
  });

  if (!response.ok) {
    throw new Error("Failed to submit transaction");
  }

  return response.json();
}

export async function getTransaction(
  signatureHash: string
): Promise<RollupTransaction> {
  const response = await fetch(`${ROLLUP_BASE_URL}/get_transaction`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ get_tx: signatureHash }),
  });

  if (!response.ok) {
    throw new Error("Failed to get transaction");
  }

  return response.json();
}

export async function getTransactionsPage(
  page: number,
  perPage: number
): Promise<RollupTransactionsList> {
  const response = await fetch(`${ROLLUP_BASE_URL}/get_transaction`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ page, per_page: perPage }),
  });

  if (!response.ok) {
    throw new Error("Failed to get transactions page");
  }

  return response.json();
}

export interface BatchSubmissionResult {
  success: boolean;
  results: Array<{ [key: string]: string } | { error: string }>;
  settlement_triggered: boolean;
  total_submitted: number;
}

export async function submitBatchTransactions(
  senderName: string | null,
  transactions: string[],
  onProgress?: (completed: number, total: number) => void
): Promise<BatchSubmissionResult> {
  const results: Array<{ [key: string]: string } | { error: string }> = [];
  let successCount = 0;

  for (let i = 0; i < transactions.length; i++) {
    try {
      const result = await submitTransaction(senderName, transactions[i]);
      results.push(result);
      successCount++;
      
      if (onProgress) {
        onProgress(i + 1, transactions.length);
      }
      
      // Small delay between submissions to avoid overwhelming the server
      await new Promise(resolve => setTimeout(resolve, 100));
    } catch (error) {
      results.push({ 
        error: error instanceof Error ? error.message : "Unknown error" 
      });
    }
  }

  return {
    success: successCount === transactions.length,
    results,
    settlement_triggered: transactions.length >= 3,
    total_submitted: successCount
  };
}
