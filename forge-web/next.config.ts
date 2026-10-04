import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // Next 16 blocks cross-origin dev requests from unlisted hosts, which breaks
  // the console's own API calls when it is reached over 127.0.0.1 or the LAN
  // address rather than localhost.
  allowedDevOrigins: ["127.0.0.1", "localhost", "192.168.1.8"],
};

export default nextConfig;