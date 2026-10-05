import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // Static export: the console ships as plain files that the Rust binary embeds
  // and serves, so a deployment is one executable with no Node runtime beside it.
  //
  // Everything the console does is client-side - 35 of 36 pages are `"use client"`
  // components that fetch from the API on mount - so there is no render-time data
  // to fetch and nothing here needs a server. The dynamic routes (`/jobs/[id]`
  // and friends) unwrap their params with React's `use()`, which exports as a
  // single shell that the client router resolves, so `generateStaticParams` is
  // neither needed nor possible: the ids are UUIDs.
  output: "export",

  // Next 16 blocks cross-origin dev requests from unlisted hosts, which breaks
  // the console's own API calls when it is reached over 127.0.0.1 or the LAN
  // address rather than localhost.
  allowedDevOrigins: ["127.0.0.1", "localhost", "192.168.1.8"],
};

export default nextConfig;