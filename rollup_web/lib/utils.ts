import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function formatLamports(lamports: number): string {
  const sol = lamports / 1_000_000_000; // LAMPORTS_PER_SOL
  return `${lamports.toLocaleString()} lamports (~${sol.toFixed(4)} SOL)`;
}

export function truncateAddress(address: string, chars = 4): string {
  if (address.length <= chars * 2) return address;
  return `${address.slice(0, chars)}...${address.slice(-chars)}`;
}

export function formatTimestamp(timestamp: number): string {
  return new Date(timestamp * 1000).toLocaleString();
}
