import type { Metadata } from "next";
import "./globals.css";
import { GeistSans } from "geist/font/sans";
import { GeistMono } from "geist/font/mono";

export const metadata: Metadata = {
  title: "ZKSVM Rollup Client",
  description: "Web interface for ZKSVM rollup transactions",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en">
      <body
        className={`${GeistSans.variable} ${GeistMono.variable} antialiased bg-background text-foreground`}
      >
        <div className="min-h-screen bg-gradient-to-br from-[hsl(var(--gradient-start))] to-[hsl(var(--gradient-end))] relative overflow-hidden">
          {/* Grid pattern */}
          <div className="absolute inset-0 bg-grid-slate-900/[0.04] bg-[size:75px_75px] dark:bg-grid-slate-400/[0.05]" />
          
          {/* Animated floating orbs */}
          <div className="absolute top-20 left-20 w-72 h-72 bg-gradient-to-r from-blue-400/10 to-purple-500/10 rounded-full blur-3xl animate-pulse" />
          <div className="absolute top-40 right-20 w-96 h-96 bg-gradient-to-r from-green-400/10 to-blue-500/10 rounded-full blur-3xl animate-pulse delay-1000" />
          <div className="absolute bottom-20 left-1/3 w-80 h-80 bg-gradient-to-r from-orange-400/10 to-pink-500/10 rounded-full blur-3xl animate-pulse delay-2000" />
          
          {/* Subtle noise texture */}
          <div className="absolute inset-0 opacity-[0.015] dark:opacity-[0.025] bg-noise" />
          
          <div className="relative z-10">
            {children}
          </div>
        </div>
      </body>
    </html>
  );
}
