"use client";

import { useState } from "react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { submitBatchTransactions, type BatchSubmissionResult } from "@/lib/api";
import { 
  Connection, 
  PublicKey, 
  Transaction, 
  SystemProgram
} from "@solana/web3.js";

declare global {
  interface Window {
    solana?: {
      isPhantom?: boolean;
      connect: () => Promise<{ publicKey: PublicKey }>;
      disconnect: () => Promise<void>;
      signTransaction: (transaction: Transaction) => Promise<Transaction>;
      publicKey?: PublicKey;
      isConnected?: boolean;
    };
  }
}

interface BatchTransactionCreatorProps {
  onTransactionSubmitted: () => void;
  walletConnected: boolean;
  walletAddress: string;
  senderName: string;
}

export function BatchTransactionCreator({
  onTransactionSubmitted,
  walletConnected,
  walletAddress: _walletAddress, // Prefix with _ to avoid unused warning
  senderName,
}: BatchTransactionCreatorProps) {
  const [recipients, setRecipients] = useState<string[]>(["", "", ""]);
  const [amounts, setAmounts] = useState<string[]>(["", "", ""]);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [submitResult, setSubmitResult] = useState<BatchSubmissionResult | null>(null);
  const [progress, setProgress] = useState({ completed: 0, total: 0 });

  const updateRecipient = (index: number, value: string) => {
    const newRecipients = [...recipients];
    newRecipients[index] = value;
    setRecipients(newRecipients);
  };

  const updateAmount = (index: number, value: string) => {
    const newAmounts = [...amounts];
    newAmounts[index] = value;
    setAmounts(newAmounts);
  };

  const validateInputs = () => {
    for (let i = 0; i < 3; i++) {
      if (!recipients[i].trim() || !amounts[i].trim()) {
        alert(`Please fill in recipient ${i + 1} address and amount`);
        return false;
      }

      try {
        new PublicKey(recipients[i].trim());
      } catch {
        alert(`Invalid recipient address for transaction ${i + 1}`);
        return false;
      }

      const amount = parseInt(amounts[i].trim());
      if (isNaN(amount) || amount <= 0) {
        alert(`Invalid amount for transaction ${i + 1}. Must be a positive number.`);
        return false;
      }
    }
    return true;
  };

  const handleBatchSubmit = async () => {
    if (!walletConnected) {
      alert("Please connect your Phantom wallet first");
      return;
    }

    if (!validateInputs()) {
      return;
    }

    setIsSubmitting(true);
    setSubmitResult(null);
    setProgress({ completed: 0, total: 3 });

    try {
      const connection = new Connection("https://api.devnet.solana.com", "confirmed");
      
      if (!window.solana || !window.solana.publicKey) {
        throw new Error("Phantom wallet not connected");
      }

      // Create all three transactions
      const transactions: string[] = [];
      const { blockhash } = await connection.getLatestBlockhash();

      for (let i = 0; i < 3; i++) {
        const recipientPubkey = new PublicKey(recipients[i].trim());
        const lamports = parseInt(amounts[i].trim());

        const transferInstruction = SystemProgram.transfer({
          fromPubkey: window.solana.publicKey,
          toPubkey: recipientPubkey,
          lamports: lamports,
        });

        const transaction = new Transaction();
        transaction.recentBlockhash = blockhash;
        transaction.feePayer = window.solana.publicKey;
        transaction.add(transferInstruction);

        const signedTransaction = await window.solana.signTransaction(transaction);
        const serializedTransaction = signedTransaction.serialize();
        const base64Transaction = serializedTransaction.toString('base64');
        
        transactions.push(base64Transaction);
      }

      // Submit all transactions as a batch
      const result = await submitBatchTransactions(
        senderName,
        transactions,
        (completed, total) => setProgress({ completed, total })
      );

      setSubmitResult(result);
      onTransactionSubmitted();

      // Reset form on success
      if (result.success) {
        setRecipients(["", "", ""]);
        setAmounts(["", "", ""]);
      }
    } catch (error) {
      console.error("Batch transaction error:", error);
      setSubmitResult({
        success: false,
        results: [{ error: error instanceof Error ? error.message : "Unknown error" }],
        settlement_triggered: false,
        total_submitted: 0
      });
    } finally {
      setIsSubmitting(false);
      setProgress({ completed: 0, total: 0 });
    }
  };

  const getProgressPercentage = () => {
    if (progress.total === 0) return 0;
    return (progress.completed / progress.total) * 100;
  };

  return (
    <Card className="bg-white/60 dark:bg-black/60 backdrop-blur-sm border-black/10 dark:border-white/20 shadow-xl">
      <CardHeader className="pb-4">
        <CardTitle className="flex items-center gap-3 text-xl font-semibold">
          <div className="p-2 bg-gradient-to-r from-green-100 to-blue-100 dark:from-green-900/30 dark:to-blue-900/30 rounded-lg">
            <svg className="w-5 h-5 text-green-600 dark:text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 10V3L4 14h7v7l9-11h-7z" />
            </svg>
          </div>
          Batch Settlement (3 Transactions)
        </CardTitle>
        <CardDescription className="text-base">
          Create 3 transactions at once to trigger immediate rollup settlement to L1
        </CardDescription>
        {walletConnected && (
          <div className="mt-2 p-3 bg-gradient-to-r from-green-50 to-blue-50 dark:from-green-900/20 dark:to-blue-900/20 rounded-lg border border-green-200 dark:border-green-800">
            <div className="flex items-center gap-2">
              <div className="w-2 h-2 bg-green-500 rounded-full animate-pulse" />
              <span className="text-sm font-medium text-green-800 dark:text-green-200">
                Settlement Mode Active
              </span>
            </div>
            <p className="text-xs text-green-600 dark:text-green-300 mt-1">
              Submitting 3 transactions will trigger L1 settlement automatically
            </p>
          </div>
        )}
      </CardHeader>

      <CardContent className="space-y-6">
        {!walletConnected ? (
          <div className="text-center py-8">
            <div className="inline-flex items-center justify-center w-16 h-16 bg-muted/50 rounded-full mb-4">
              <svg className="w-8 h-8 text-muted-foreground" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
              </svg>
            </div>
            <h3 className="text-lg font-semibold mb-2">Wallet Required</h3>
            <p className="text-muted-foreground">
              Connect your Phantom wallet to use batch settlement
            </p>
          </div>
        ) : (
          <>
            <div className="space-y-4">
              {[0, 1, 2].map((index) => (
                <div key={index} className="p-4 bg-muted/30 rounded-lg border border-black/5 dark:border-white/10">
                  <h4 className="font-semibold mb-3 flex items-center gap-2">
                    <div className={`w-6 h-6 rounded-full flex items-center justify-center text-xs font-bold ${
                      index === 0 ? 'bg-blue-100 text-blue-600 dark:bg-blue-900/50 dark:text-blue-400' :
                      index === 1 ? 'bg-green-100 text-green-600 dark:bg-green-900/50 dark:text-green-400' :
                      'bg-purple-100 text-purple-600 dark:bg-purple-900/50 dark:text-purple-400'
                    }`}>
                      {index + 1}
                    </div>
                    Transaction {index + 1}
                  </h4>
                  
                  <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                    <div>
                      <label className="text-sm font-medium mb-2 block">
                        Recipient Address
                      </label>
                      <Input
                        placeholder="e.g. 9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM"
                        value={recipients[index]}
                        onChange={(e) => updateRecipient(index, e.target.value)}
                        className="font-mono text-sm"
                      />
                    </div>
                    
                    <div>
                      <label className="text-sm font-medium mb-2 block">
                        Amount (lamports)
                      </label>
                      <Input
                        type="number"
                        placeholder="e.g. 1000000"
                        value={amounts[index]}
                        onChange={(e) => updateAmount(index, e.target.value)}
                      />
                    </div>
                  </div>
                </div>
              ))}
            </div>

            {isSubmitting && (
              <div className="p-4 bg-blue-50 dark:bg-blue-900/20 rounded-lg border border-blue-200 dark:border-blue-800">
                <div className="flex items-center justify-between mb-2">
                  <span className="text-sm font-medium text-blue-800 dark:text-blue-200">
                    Submitting Batch Transactions...
                  </span>
                  <span className="text-xs text-blue-600 dark:text-blue-300">
                    {progress.completed}/{progress.total}
                  </span>
                </div>
                <div className="w-full bg-blue-200 dark:bg-blue-800 rounded-full h-2">
                  <div
                    className="bg-blue-600 dark:bg-blue-400 h-2 rounded-full transition-all duration-300"
                    style={{ width: `${getProgressPercentage()}%` }}
                  />
                </div>
              </div>
            )}

            <Button
              onClick={handleBatchSubmit}
              disabled={isSubmitting}
              className="w-full h-12 text-base font-medium bg-gradient-to-r from-green-600 to-blue-600 hover:from-green-700 hover:to-blue-700"
            >
              {isSubmitting ? (
                <div className="flex items-center gap-2">
                  <div className="animate-spin rounded-full h-4 w-4 border-b-2 border-white" />
                  Submitting {progress.completed}/3 Transactions...
                </div>
              ) : (
                <div className="flex items-center gap-2">
                  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 10V3L4 14h7v7l9-11h-7z" />
                  </svg>
                  Submit Batch & Trigger Settlement
                </div>
              )}
            </Button>

            {submitResult && (
              <div className={`p-4 rounded-lg border ${
                submitResult.success 
                  ? 'bg-green-50 dark:bg-green-900/20 border-green-200 dark:border-green-800' 
                  : 'bg-red-50 dark:bg-red-900/20 border-red-200 dark:border-red-800'
              }`}>
                <div className="flex items-center gap-2 mb-2">
                  {submitResult.success ? (
                    <svg className="w-5 h-5 text-green-600 dark:text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                    </svg>
                  ) : (
                    <svg className="w-5 h-5 text-red-600 dark:text-red-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                    </svg>
                  )}
                  <h4 className="font-medium">
                    {submitResult.success ? 'Batch Submitted Successfully!' : 'Batch Submission Failed'}
                  </h4>
                </div>
                
                <div className="space-y-2 text-sm">
                  <p>Transactions submitted: {submitResult.total_submitted}/3</p>
                  {submitResult.settlement_triggered && (
                    <p className="text-green-600 dark:text-green-400 font-medium">
                      🎉 L1 Settlement triggered! Your transactions will be settled on Solana.
                    </p>
                  )}
                </div>

                <details className="mt-3">
                  <summary className="cursor-pointer text-sm font-medium opacity-70 hover:opacity-100">
                    View detailed results
                  </summary>
                  <pre className="text-xs mt-2 p-2 bg-background/50 rounded border overflow-x-auto">
                    {JSON.stringify(submitResult, null, 2)}
                  </pre>
                </details>
              </div>
            )}
          </>
        )}
      </CardContent>
    </Card>
  );
}