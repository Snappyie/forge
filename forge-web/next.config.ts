import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // Allow remote testing via local network IP
  // Allow remote testing via local network IP
  // @ts-expect-error - Next.js config might not strictly type this in some versions
  allowedDevOrigins: ['192.168.1.8'],
};

export default nextConfig;
