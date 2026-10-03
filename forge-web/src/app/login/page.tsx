"use client";

import { motion } from "framer-motion";
import { Card, CardContent, CardHeader, CardTitle, CardDescription, CardFooter } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Hexagon } from "lucide-react";
import Link from "next/link";

export default function LoginPage() {
  return (
    <div className="min-h-screen w-full flex items-center justify-center bg-[radial-gradient(ellipse_at_top_right,_var(--tw-gradient-stops))] from-indigo-50/50 via-background to-background dark:from-indigo-900/20 dark:via-background dark:to-background">
      <motion.div
        initial={{ opacity: 0, scale: 0.95 }}
        animate={{ opacity: 1, scale: 1 }}
        transition={{ duration: 0.3 }}
        className="w-full max-w-md p-4"
      >
        <div className="flex justify-center mb-8">
          <div className="flex items-center gap-2">
            <div className="bg-indigo-600 p-2 rounded-lg">
              <Hexagon className="h-6 w-6 text-white" />
            </div>
            <span className="text-2xl font-bold tracking-tight">Forge</span>
          </div>
        </div>

        <Card className="bg-card/50 backdrop-blur-xl border-border/50 shadow-2xl">
          <CardHeader className="space-y-1">
            <CardTitle className="text-2xl text-center">Welcome back</CardTitle>
            <CardDescription className="text-center">
              Enter your email to sign in to your account
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="space-y-2">
              <label className="text-sm font-medium leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70" htmlFor="email">
                Email
              </label>
              <Input id="email" type="email" placeholder="m@example.com" className="bg-background/50" />
            </div>
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <label className="text-sm font-medium leading-none" htmlFor="password">
                  Password
                </label>
                <Link href="#" className="text-xs text-indigo-500 hover:text-indigo-400">
                  Forgot password?
                </Link>
              </div>
              <Input id="password" type="password" className="bg-background/50" />
            </div>
          </CardContent>
          <CardFooter className="flex flex-col gap-4">
            <Button className="w-full bg-indigo-600 hover:bg-indigo-700 text-white">Sign In</Button>
            <div className="text-sm text-center text-muted-foreground">
              Don&apos;t have an account?{" "}
              <Link href="/register" className="text-indigo-500 hover:text-indigo-400 font-medium">
                Sign up
              </Link>
            </div>
          </CardFooter>
        </Card>
      </motion.div>
    </div>
  );
}
