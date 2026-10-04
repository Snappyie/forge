import type { Metadata } from "next";
import { Inter, JetBrains_Mono } from "next/font/google";
import { AuthProvider } from "@/lib/auth";
import { ShellProvider } from "@/lib/shell";
import { AppShell } from "@/components/layout/AppShell";
import { Toaster } from "@/components/ui/toast";
import "./globals.css";

const inter = Inter({
  variable: "--font-sans",
  subsets: ["latin"],
  display: "swap",
});

const jetbrainsMono = JetBrains_Mono({
  variable: "--font-mono",
  subsets: ["latin"],
  display: "swap",
});

export const metadata: Metadata = {
  title: "Forge | Execution Engine",
  description: "Distributed job orchestration platform",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    // No `dark` class here: `ShellProvider` applies it, so the theme selector
    // can actually switch palettes without a full reload.
    <html lang="en" className={`${inter.variable} ${jetbrainsMono.variable} h-full antialiased font-sans`}>
      <body className="min-h-full bg-background text-foreground">
        {/*
          The shell lives here rather than in the root body so the sign-in and
          register pages can opt out of the sidebar and header.
        */}
        <AuthProvider>
          <ShellProvider>
            {/*
              `Toaster` supplies the toast manager context that `useToast`
              consumes, so it must wrap the app rather than sit beside it.
            */}
            <Toaster>
              <AppShell>{children}</AppShell>
            </Toaster>
          </ShellProvider>
        </AuthProvider>
      </body>
    </html>
  );
}
