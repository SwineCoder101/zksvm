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
import { submitTransaction } from "@/lib/api";
import { 
  Connection, 
  PublicKey, 
  Transaction, 
  SystemProgram
} from "@solana/web3.js";

// Phantom wallet interface
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

interface TransactionCreatorProps {
  onTransactionSubmitted: () => void;
  walletConnected: boolean;
  walletAddress: string;
  senderName: string;
  onWalletConnect: (connected: boolean, address: string, name: string) => void;
}

export function TransactionCreator({
  onTransactionSubmitted,
  walletConnected,
  walletAddress,
  senderName,
  onWalletConnect,
}: TransactionCreatorProps) {
  const [recipientAddress, setRecipientAddress] = useState("");
  const [amount, setAmount] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [submitResult, setSubmitResult] = useState<Record<
    string,
    unknown
  > | null>(null);

  const connectWallet = async () => {
    try {
      if (!window.solana || !window.solana.isPhantom) {
        alert("Please install Phantom wallet");
        return;
      }

      const resp = await window.solana.connect();
      onWalletConnect(true, resp.publicKey.toString(), "Phantom User");
    } catch (error) {
      console.error("Failed to connect wallet:", error);
      alert("Failed to connect to Phantom wallet");
    }
  };

  const disconnectWallet = async () => {
    try {
      if (window.solana) {
        await window.solana.disconnect();
      }
      onWalletConnect(false, "", "");
    } catch (error) {
      console.error("Failed to disconnect wallet:", error);
    }
  };

  const handleSubmit = async () => {
    if (!walletConnected) {
      alert("Please connect your Phantom wallet first");
      return;
    }

    if (!senderName.trim() || !recipientAddress.trim() || !amount.trim()) {
      alert("Please fill in all fields");
      return;
    }

    setIsSubmitting(true);
    setSubmitResult(null);

    try {
      // Connect to Solana devnet
      const connection = new Connection("https://api.devnet.solana.com", "confirmed");
      
      if (!window.solana || !window.solana.publicKey) {
        throw new Error("Phantom wallet not connected");
      }
      
      // Parse recipient address
      let recipientPubkey: PublicKey;
      try {
        recipientPubkey = new PublicKey(recipientAddress.trim());
      } catch {
        throw new Error("Invalid recipient address - must be a valid Solana pubkey");
      }
      
      // Parse amount
      const lamports = parseInt(amount.trim());
      if (isNaN(lamports) || lamports <= 0) {
        throw new Error("Amount must be a positive number");
      }
      
      // Get recent blockhash from devnet
      const { blockhash } = await connection.getLatestBlockhash();
      
      // Create transfer instruction
      const transferInstruction = SystemProgram.transfer({
        fromPubkey: window.solana.publicKey,
        toPubkey: recipientPubkey,
        lamports: lamports,
      });
      
      // Create transaction
      const transaction = new Transaction();
      transaction.recentBlockhash = blockhash;
      transaction.feePayer = window.solana.publicKey;
      transaction.add(transferInstruction);
      
      // Sign the transaction with Phantom
      const signedTransaction = await window.solana.signTransaction(transaction);
      
      // Serialize the signed transaction to base64 - this is the proper format
      const serializedTransaction = signedTransaction.serialize();
      const base64Transaction = serializedTransaction.toString('base64');
      
      console.log("Signed transaction (base64):", base64Transaction);
      
      // Send the serialized transaction as a base64 string
      const result = await submitTransaction(senderName, base64Transaction);
      setSubmitResult(result);
      onTransactionSubmitted();

      // Reset form (except wallet info)
      setRecipientAddress("");
      setAmount("");
    } catch (error) {
      console.error("Transaction error:", error);
      setSubmitResult({
        error: error instanceof Error ? error.message : "Unknown error",
      });
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <Card className="bg-white/60 dark:bg-black/60 backdrop-blur-sm border-black/10 dark:border-white/20 shadow-xl">
      <CardHeader className="pb-4">
        <CardTitle className="flex items-center gap-3 text-xl font-semibold">
          <div className="p-2 bg-blue-100 dark:bg-blue-900/30 rounded-lg">
            <svg className="w-5 h-5 text-blue-600 dark:text-blue-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 6v6m0 0v6m0-6h6m-6 0H6" />
            </svg>
          </div>
          Create Transaction
        </CardTitle>
        <CardDescription className="text-base">
          Connect your wallet and submit new transactions to the ZKSVM rollup
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        {!walletConnected ? (
          <div className="space-y-4">
            <div className="text-center">
              <p className="text-sm text-gray-600 mb-4">
                Connect your Phantom wallet to create real transactions
              </p>
              <Button onClick={connectWallet} className="w-full">
                Connect Phantom Wallet
              </Button>
            </div>
          </div>
        ) : (
          <>
            <div className="bg-green-50 dark:bg-green-900/20 p-3 rounded-md">
              <p className="text-sm font-medium text-green-800 dark:text-green-200">
                ✅ Wallet Connected
              </p>
              <p className="text-xs text-green-600 dark:text-green-300 mt-1">
                {walletAddress}
              </p>
              <Button onClick={disconnectWallet} variant="outline" size="sm" className="mt-2">
                Disconnect
              </Button>
            </div>
            <div>
              <label className="text-sm font-medium mb-2 block">Sender Name</label>
              <Input
                placeholder="Connected wallet user"
                value={senderName}
                readOnly
                className="bg-muted/50 cursor-not-allowed"
              />
            </div>
          </>
        )}

        <div>
          <label className="text-sm font-medium mb-2 block">
            Recipient Address (Solana Pubkey)
          </label>
          <Input
            placeholder="e.g. 9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM"
            value={recipientAddress}
            onChange={(e) => setRecipientAddress(e.target.value)}
          />
          <p className="text-xs text-gray-500 mt-1">
            Must be a valid base58 Solana public key (44 characters)
          </p>
        </div>

        <div>
          <label className="text-sm font-medium mb-2 block">
            Amount (lamports)
          </label>
          <Input
            type="number"
            placeholder="e.g. 1000000 (= 0.001 SOL)"
            value={amount}
            onChange={(e) => setAmount(e.target.value)}
          />
          <p className="text-xs text-gray-500 mt-1">
            1 SOL = 1,000,000,000 lamports
          </p>
        </div>

        {walletConnected && (
          <Button
            onClick={handleSubmit}
            disabled={isSubmitting}
            className="w-full flex items-center gap-2"
          >
            {isSubmitting ? "Submitting..." : "Submit Transaction"}
          </Button>
        )}

        {submitResult && (
          <div className="mt-4 p-4 bg-gray-50 dark:bg-gray-800 rounded-md">
            <h4 className="font-medium mb-2">Result:</h4>
            <pre className="text-sm text-gray-800 dark:text-gray-200 whitespace-pre-wrap">
              {JSON.stringify(submitResult, null, 2)}
            </pre>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
