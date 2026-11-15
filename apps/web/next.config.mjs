/** @type {import('next').NextConfig} */
const nextConfig = {
  output: "standalone",
  poweredByHeader: false,
  async rewrites() {
    const backend = process.env.AXION_API_URL ?? "http://127.0.0.1:8080";
    return [{ source: "/api/:path*", destination: `${backend}/:path*` }];
  },
};

export default nextConfig;

