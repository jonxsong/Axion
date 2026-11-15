import type { Metadata } from "next";
import { Geist, Geist_Mono } from "next/font/google";
import type { ReactNode } from "react";

import "./globals.css";

const geist = Geist({ variable: "--font-geist", subsets: ["latin"] });
const mono = Geist_Mono({ variable: "--font-mono", subsets: ["latin"] });

export const metadata: Metadata = {
  title: { default: "Axion — Vector search from first principles", template: "%s · Axion" },
  description:
    "A high-performance vector search engine built in Rust for fast similarity search, durable storage, and developer control.",
  metadataBase: new URL("https://github.com/jonxsong/Axion"),
  openGraph: {
    title: "Axion",
    description: "Vector search, engineered from first principles.",
    type: "website",
  },
  icons: { icon: "/favicon.svg" },
};

export default function RootLayout({ children }: Readonly<{ children: ReactNode }>) {
  return (
    <html lang="en" className={`${geist.variable} ${mono.variable}`}>
      <body>{children}</body>
    </html>
  );
}

