import { forwardRef } from "react";
import type { usePlayback } from "../hooks/usePlayback";
import MapGL, { type MapHandle, type RoutePath } from "./MapGL";

type PB = ReturnType<typeof usePlayback>;

interface Props {
  pb: PB;
  routes: RoutePath[];
  showRoutes: boolean;
  showWeather: boolean;
  showLegend: boolean;
}

/** The map stage — a clean, full-height GL map. All chrome lives in the left
 *  panel; here we render only the deck.gl data overlays (+ the compact legend). */
const MapPane = forwardRef<MapHandle, Props>(function MapPane(
  { pb, routes, showRoutes, showWeather, showLegend }, ref,
) {
  const t = pb.currentTick;
  return (
    <div style={{ position: "absolute", inset: 0 }}>
      <MapGL
        ref={ref}
        vessels={t?.vessels ?? []}
        ports={t?.ports ?? []}
        routes={routes}
        showRoutes={showRoutes}
        showLegend={showLegend}
        collisionEvents={t?.collision_events ?? []}
        currentStep={t?.step ?? 0}
        rescueAgents={t?.rescue_agents ?? []}
        wrecks={t?.wrecks ?? []}
        mobPersons={t?.mob_persons ?? []}
        mobAgents={t?.mob_agents ?? []}
        storm={t?.storm ?? null}
        weatherGrid={t?.weather_grid ?? []}
        weatherGridSize={t?.weather_grid_size ?? 0}
        showWeather={showWeather}
        latMin={t?.bbox?.lat_min}
        latMax={t?.bbox?.lat_max}
        lonMin={t?.bbox?.lon_min}
        lonMax={t?.bbox?.lon_max}
        transitionMs={pb.speedMs}
      />
    </div>
  );
});

export default MapPane;
