"use client";

import { useState, useEffect } from "react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { TransactionCreator } from "@/components/transaction-creator";
import { BatchTransactionCreator } from "@/components/batch-transaction-creator";
import {
  healthCheck,
  getTransaction,
  getTransactionsPage,
  type TransactionWithHash,
  type RollupTransaction,
} from "@/lib/api";

interface HealthStatus {
  status: string;
  timestamp: number;
}


export default function RollupClientPage() {
  const [healthStatus, setHealthStatus] = useState<HealthStatus | null>(null);
  const [isHealthLoading, setIsHealthLoading] = useState(false);
  const [transactions, setTransactions] = useState<TransactionWithHash[]>([]);
  const [isTransactionsLoading, setIsTransactionsLoading] = useState(false);
  const [currentPage, setCurrentPage] = useState(1);
  const [transactionHash, setTransactionHash] = useState("");
  const [searchResult, setSearchResult] = useState<RollupTransaction | null>(
    null
  );
  const [isSearchLoading, setIsSearchLoading] = useState(false);
  
  // Shared wallet state
  const [walletConnected, setWalletConnected] = useState(false);
  const [walletAddress, setWalletAddress] = useState<string>("");
  const [senderName, setSenderName] = useState("");

  // Wallet connection handler
  const handleWalletConnect = (connected: boolean, address: string, name: string) => {
    setWalletConnected(connected);
    setWalletAddress(address);
    setSenderName(name);
  };

  // Check wallet connection on load
  useEffect(() => {
    if (window.solana && window.solana.isConnected && window.solana.publicKey) {
      setWalletConnected(true);
      setWalletAddress(window.solana.publicKey.toString());
      setSenderName("Phantom User");
    }
  }, []);

  // Health check function
  const performHealthCheck = async () => {
    setIsHealthLoading(true);
    try {
      const result = await healthCheck();
      setHealthStatus({
        status: JSON.stringify(result),
        timestamp: Date.now(),
      });
    } catch (error) {
      setHealthStatus({
        status: `Error: ${
          error instanceof Error ? error.message : "Unknown error"
        }`,
        timestamp: Date.now(),
      });
    } finally {
      setIsHealthLoading(false);
    }
  };

  // Load transactions
  const loadTransactions = async (page = 1) => {
    setIsTransactionsLoading(true);
    try {
      const result = await getTransactionsPage(page, 10);
      setTransactions(result.transactions);
      setCurrentPage(page);
    } catch (error) {
      console.error("Failed to load transactions:", error);
    } finally {
      setIsTransactionsLoading(false);
    }
  };

  // Search for specific transaction
  const searchTransaction = async () => {
    if (!transactionHash.trim()) return;

    setIsSearchLoading(true);
    try {
      const result = await getTransaction(transactionHash.trim());
      setSearchResult(result);
    } catch (error) {
      setSearchResult({
        error: error instanceof Error ? error.message : "Unknown error",
      });
    } finally {
      setIsSearchLoading(false);
    }
  };

  // Load initial data
  useEffect(() => {
    performHealthCheck();
    loadTransactions();
  }, []);

  return (
    <div className="container mx-auto py-12 px-4 max-w-7xl">
      {/* Hero Section */}
      <div className="text-center mb-16">
        <div className="inline-flex items-center bg-black/5 dark:bg-white/10 backdrop-blur-sm rounded-full px-4 py-2 text-sm font-medium mb-6 border border-black/10 dark:border-white/20">
          <div className="w-2 h-2 bg-green-500 rounded-full mr-2 animate-pulse" />
          ZKSVM Rollup Network
        </div>
        <h1 className="text-6xl font-bold tracking-tight mb-6 bg-gradient-to-r from-black to-gray-600 dark:from-white dark:to-gray-300 bg-clip-text text-transparent">
          Zero-Knowledge Rollup
          <br />
          <span className="text-5xl">Management Dashboard</span>
        </h1>
        <p className="text-xl text-muted-foreground max-w-2xl mx-auto leading-relaxed">
          Monitor, create, and analyze zero-knowledge rollup transactions with our comprehensive web interface. 
          <span className="text-primary font-semibold"> Create batch transactions to trigger instant L1 settlement.</span>
        </p>
        <div className="flex flex-col items-center gap-4 mt-8">
          <div className="flex items-center gap-4">
            <Button 
              onClick={performHealthCheck} 
              disabled={isHealthLoading}
              size="lg"
              className="px-8 py-3 text-base font-medium"
            >
              {isHealthLoading ? "Checking..." : "Health Check"}
            </Button>
            <Button 
              onClick={() => loadTransactions(currentPage)}
              disabled={isTransactionsLoading}
              variant="outline" 
              size="lg"
              className="px-8 py-3 text-base font-medium"
            >
              View Transactions
            </Button>
          </div>
          <div className="flex items-center gap-2 text-sm text-muted-foreground mt-2">
            <div className="flex items-center gap-1">
              <div className="w-2 h-2 bg-blue-500 rounded-full" />
              <span>Single Transactions</span>
            </div>
            <div className="w-px h-4 bg-muted-foreground/30" />
            <div className="flex items-center gap-1">
              <div className="w-2 h-2 bg-gradient-to-r from-green-500 to-blue-500 rounded-full animate-pulse" />
              <span>Batch Settlement (3 tx → L1)</span>
            </div>
          </div>
        </div>
      </div>

      {/* Stats Grid */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-6 mb-12">
        <Card className="bg-white/50 dark:bg-black/50 backdrop-blur-sm border-black/10 dark:border-white/20">
          <CardContent className="p-6">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm font-medium text-muted-foreground">Network Status</p>
                <div className="flex items-center gap-2 mt-2">
                  <div className={`w-3 h-3 rounded-full ${healthStatus?.status.includes('Error') ? 'bg-red-500' : 'bg-green-500'} animate-pulse`} />
                  <span className="text-2xl font-bold">{healthStatus?.status.includes('Error') ? 'Offline' : 'Online'}</span>
                </div>
              </div>
              <div className="p-3 bg-primary/10 rounded-full">
                <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 10V3L4 14h7v7l9-11h-7z" />
                </svg>
              </div>
            </div>
          </CardContent>
        </Card>
        
        <Card className="bg-white/50 dark:bg-black/50 backdrop-blur-sm border-black/10 dark:border-white/20">
          <CardContent className="p-6">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm font-medium text-muted-foreground">Total Transactions</p>
                <p className="text-2xl font-bold mt-2">{transactions.length}</p>
              </div>
              <div className="p-3 bg-primary/10 rounded-full">
                <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
                </svg>
              </div>
            </div>
          </CardContent>
        </Card>
        
        <Card className="bg-white/50 dark:bg-black/50 backdrop-blur-sm border-black/10 dark:border-white/20">
          <CardContent className="p-6">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm font-medium text-muted-foreground">Current Page</p>
                <p className="text-2xl font-bold mt-2">{currentPage}</p>
              </div>
              <div className="p-3 bg-primary/10 rounded-full">
                <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 4V2a1 1 0 011-1h8a1 1 0 011 1v2h4a1 1 0 110 2h-1v14a2 2 0 01-2 2H6a2 2 0 01-2-2V6H3a1 1 0 110-2h4z" />
                </svg>
              </div>
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Transaction Creation Section */}
      <div className="space-y-8 mb-8">
        {/* Batch Transaction Creator - Full Width */}
        <BatchTransactionCreator
          onTransactionSubmitted={() => loadTransactions(currentPage)}
          walletConnected={walletConnected}
          walletAddress={walletAddress}
          senderName={senderName}
        />

        {/* Main Grid */}
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-8">
          {/* Health Check Card */}
          <Card className="bg-white/60 dark:bg-black/60 backdrop-blur-sm border-black/10 dark:border-white/20 shadow-xl">
            <CardHeader className="pb-4">
              <CardTitle className="flex items-center gap-3 text-xl font-semibold">
                <div className="p-2 bg-green-100 dark:bg-green-900/30 rounded-lg">
                  <svg className="w-5 h-5 text-green-600 dark:text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4.318 6.318a4.5 4.5 0 000 6.364L12 20.364l7.682-7.682a4.5 4.5 0 00-6.364-6.364L12 7.636l-1.318-1.318a4.5 4.5 0 00-6.364 0z" />
                  </svg>
                </div>
                System Health
              </CardTitle>
              <CardDescription className="text-base">
                Monitor rollup server connectivity and performance metrics
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <Button
                onClick={performHealthCheck}
                disabled={isHealthLoading}
                className="w-full h-12 text-base font-medium"
              >
                {isHealthLoading ? "Checking System Health..." : "Run Health Check"}
              </Button>
              {healthStatus && (
                <div className="p-4 bg-muted/50 backdrop-blur-sm rounded-lg border">
                  <code className="text-sm font-mono block break-all">
                    {healthStatus.status}
                  </code>
                  <div className="text-xs text-muted-foreground mt-2 flex items-center gap-2">
                    <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
                    </svg>
                    Last checked: {new Date(healthStatus.timestamp).toLocaleTimeString()}
                  </div>
                </div>
              )}
            </CardContent>
          </Card>

          {/* Single Transaction Creator */}
          <TransactionCreator
            onTransactionSubmitted={() => loadTransactions(currentPage)}
            walletConnected={walletConnected}
            walletAddress={walletAddress}
            senderName={senderName}
            onWalletConnect={handleWalletConnect}
          />
        </div>
      </div>

      {/* Transaction Search Card */}
      <Card className="mb-8 bg-white/60 dark:bg-black/60 backdrop-blur-sm border-black/10 dark:border-white/20 shadow-xl">
        <CardHeader className="pb-4">
          <CardTitle className="flex items-center gap-3 text-xl font-semibold">
            <div className="p-2 bg-purple-100 dark:bg-purple-900/30 rounded-lg">
              <svg className="w-5 h-5 text-purple-600 dark:text-purple-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
              </svg>
            </div>
            Transaction Search
          </CardTitle>
          <CardDescription className="text-base">
            Find and inspect specific transactions using their signature hash
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex items-center gap-4">
            <Input
              placeholder="Enter transaction signature hash..."
              value={transactionHash}
              onChange={(e) => setTransactionHash(e.target.value)}
              className="flex-1 h-12 text-base"
              onKeyDown={(e) => e.key === "Enter" && searchTransaction()}
            />
            <Button
              onClick={searchTransaction}
              disabled={isSearchLoading || !transactionHash.trim()}
              className="h-12 px-6 text-base font-medium"
            >
              {isSearchLoading ? "Searching..." : "Search"}
            </Button>
          </div>

          {searchResult && (
            <div className="p-4 bg-muted/50 backdrop-blur-sm rounded-lg border">
              <pre className="text-sm font-mono whitespace-pre-wrap break-all">
                {JSON.stringify(searchResult, null, 2)}
              </pre>
            </div>
          )}
        </CardContent>
      </Card>

      {/* Transactions List Card */}
      <Card className="bg-white/60 dark:bg-black/60 backdrop-blur-sm border-black/10 dark:border-white/20 shadow-xl">
        <CardHeader className="pb-4">
          <CardTitle className="flex items-center gap-3 text-xl font-semibold">
            <div className="p-2 bg-orange-100 dark:bg-orange-900/30 rounded-lg">
              <svg className="w-5 h-5 text-orange-600 dark:text-orange-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z" />
              </svg>
            </div>
            Recent Transactions
          </CardTitle>
          <CardDescription className="text-base">
            Browse and monitor the latest transactions processed by the rollup
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-6">
          <div className="flex items-center justify-between">
            <Button
              onClick={() => loadTransactions(currentPage)}
              disabled={isTransactionsLoading}
              variant="outline"
              className="h-10 px-6 font-medium"
            >
              <svg className="w-4 h-4 mr-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
              </svg>
              Refresh
            </Button>

            <div className="flex items-center gap-2">
              <Button
                onClick={() => loadTransactions(Math.max(1, currentPage - 1))}
                disabled={isTransactionsLoading || currentPage <= 1}
                variant="outline"
                size="sm"
                className="h-9 px-4"
              >
                <svg className="w-4 h-4 mr-1" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
                </svg>
                Previous
              </Button>
              <div className="px-4 py-2 bg-muted/50 rounded-lg border text-sm font-medium">
                Page {currentPage}
              </div>
              <Button
                onClick={() => loadTransactions(currentPage + 1)}
                disabled={isTransactionsLoading || transactions.length < 10}
                variant="outline"
                size="sm"
                className="h-9 px-4"
              >
                Next
                <svg className="w-4 h-4 ml-1" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
                </svg>
              </Button>
            </div>
          </div>

          {isTransactionsLoading ? (
            <div className="flex items-center justify-center py-12">
              <div className="flex items-center gap-3 text-muted-foreground">
                <div className="animate-spin rounded-full h-6 w-6 border-b-2 border-primary"></div>
                <span className="text-base font-medium">Loading transactions...</span>
              </div>
            </div>
          ) : transactions.length > 0 ? (
            <div className="space-y-4">
              {transactions.map((tx, index) => (
                <div
                  key={index}
                  className="p-4 bg-muted/30 backdrop-blur-sm rounded-lg border border-black/5 dark:border-white/10 hover:bg-muted/50 transition-colors"
                >
                  <div className="flex items-center justify-between mb-3">
                    <div className="font-mono text-sm font-medium bg-primary/10 px-3 py-1 rounded-full">
                      {tx.hash}
                    </div>
                    <div className="text-xs text-muted-foreground flex items-center gap-2">
                      <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
                      </svg>
                      Transaction #{index + 1}
                    </div>
                  </div>
                  <div className="p-3 bg-background/50 rounded-md border">
                    <pre className="text-xs font-mono whitespace-pre-wrap overflow-x-auto text-muted-foreground">
                      {JSON.stringify(tx.transaction, null, 2)}
                    </pre>
                  </div>
                </div>
              ))}
            </div>
          ) : (
            <div className="text-center py-12">
              <div className="inline-flex items-center justify-center w-16 h-16 bg-muted/50 rounded-full mb-4">
                <svg className="w-8 h-8 text-muted-foreground" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
                </svg>
              </div>
              <h3 className="text-lg font-semibold mb-2">No Transactions Found</h3>
              <p className="text-muted-foreground mb-4">Start by creating a new transaction or check back later.</p>
              <Button onClick={() => loadTransactions(currentPage)} variant="outline">
                Refresh Transactions
              </Button>
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
