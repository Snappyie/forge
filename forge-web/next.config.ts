import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // Allow remote testing via local network IP
  allowedDevOrigins: ['192.168.1.8'],
};

export default nextConfig;
