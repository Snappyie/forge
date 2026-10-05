"use client";

import { useEffect, useState } from "react";

import { EXPORT_ID } from "@/lib/export-params";
import View from "./view";

/**
 * Resolves the id segment from the URL and hands it to the detail view.
 *
 * `useParams` is the obvious tool here, and it does not work under
 * `output: "export"`: there is no server rendering the route, so a hard load of
 * `/jobs/<id>` has no params to hand down. The page booted and fetched
 * `__static_export__` rather than the id in the address bar, so every detail page
 * - including pasted and bookmarked ones - showed "This page couldn't load".
 *
 * The id is therefore read from the location after mount, which is the only place
 * it exists in a static export. Nothing is rendered until it is known, because
 * rendering the view with a placeholder is exactly what produced the misleading
 * error.
 *
 * Client-side navigation within the console re-renders this component, and the
 * effect is keyed on nothing, so a navigation to a different id would leave the
 * old one displayed. Keyed on the path to avoid that.
 */
export default function IdFromUrl() {
  const [id, setId] = useState<string | null>(null);

  useEffect(() => {
    const read = () => {
      // The id is the last segment: `/jobs/<id>`. Read it rather than index a
      // fixed position, so a trailing slash or an extra segment cannot silently
      // produce the wrong id.
      const segments = window.location.pathname.split("/").filter(Boolean);
      const fromUrl = segments.length >= 2 ? segments[segments.length - 1] : null;
      setId(fromUrl ? decodeURIComponent(fromUrl) : null);
    };

    read();
    // The export ships no router events to subscribe to, so this is cheap rather
    // than a hot path: it runs on mount and on every path change.
    window.addEventListener("popstate", read);
    return () => window.removeEventListener("popstate", read);
  }, []);

  if (!id || id === EXPORT_ID) {
    // Still resolving, or the exported shell fetched without a route segment.
    // Either way there is nothing real to show, and rendering the view with a
    // placeholder would only produce a request that cannot succeed.
    return null;
  }

  return <View id={id} />;
}
