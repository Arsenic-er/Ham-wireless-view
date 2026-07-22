import type { MapPoint } from "./types";
import type { Feature, FeatureCollection, LineString } from "geojson";

const WGS84_A_M = 6_378_137;
const WGS84_F = 1 / 298.257_223_563;
const WGS84_B_M = WGS84_A_M * (1 - WGS84_F);
const COVERAGE_RADIUS_M = 200_000;

function toRadians(degrees: number): number {
  return (degrees * Math.PI) / 180;
}

function toDegrees(radians: number): number {
  return (radians * 180) / Math.PI;
}

function normalizeLongitude(longitude: number): number {
  return ((longitude + 540) % 360) - 180;
}

export function directWgs84(
  origin: MapPoint,
  azimuthDegrees: number,
  distanceM: number,
): MapPoint {
  const alpha1 = toRadians(azimuthDegrees);
  const sinAlpha1 = Math.sin(alpha1);
  const cosAlpha1 = Math.cos(alpha1);
  const phi1 = toRadians(origin.lat);
  const tanU1 = (1 - WGS84_F) * Math.tan(phi1);
  const cosU1 = 1 / Math.sqrt(1 + tanU1 * tanU1);
  const sinU1 = tanU1 * cosU1;
  const sigma1 = Math.atan2(tanU1, cosAlpha1);
  const sinAlpha = cosU1 * sinAlpha1;
  const cosSqAlpha = 1 - sinAlpha * sinAlpha;
  const uSq =
    (cosSqAlpha * (WGS84_A_M * WGS84_A_M - WGS84_B_M * WGS84_B_M)) /
    (WGS84_B_M * WGS84_B_M);
  const bigA =
    1 +
    (uSq / 16_384) *
      (4096 + uSq * (-768 + uSq * (320 - 175 * uSq)));
  const bigB =
    (uSq / 1024) * (256 + uSq * (-128 + uSq * (74 - 47 * uSq)));

  let sigma = distanceM / (WGS84_B_M * bigA);
  let previousSigma = Number.POSITIVE_INFINITY;
  let cos2SigmaM = 0;
  let sinSigma = 0;
  let cosSigma = 0;
  for (let iteration = 0; iteration < 100; iteration += 1) {
    cos2SigmaM = Math.cos(2 * sigma1 + sigma);
    sinSigma = Math.sin(sigma);
    cosSigma = Math.cos(sigma);
    const deltaSigma =
      bigB *
      sinSigma *
      (cos2SigmaM +
        (bigB / 4) *
          (cosSigma * (-1 + 2 * cos2SigmaM * cos2SigmaM) -
            (bigB / 6) *
              cos2SigmaM *
              (-3 + 4 * sinSigma * sinSigma) *
              (-3 + 4 * cos2SigmaM * cos2SigmaM)));
    previousSigma = sigma;
    sigma = distanceM / (WGS84_B_M * bigA) + deltaSigma;
    if (Math.abs(sigma - previousSigma) < 1e-12) {
      break;
    }
  }

  const temporary = sinU1 * sinSigma - cosU1 * cosSigma * cosAlpha1;
  const phi2 = Math.atan2(
    sinU1 * cosSigma + cosU1 * sinSigma * cosAlpha1,
    (1 - WGS84_F) * Math.sqrt(sinAlpha * sinAlpha + temporary * temporary),
  );
  const lambda = Math.atan2(
    sinSigma * sinAlpha1,
    cosU1 * cosSigma - sinU1 * sinSigma * cosAlpha1,
  );
  const c =
    (WGS84_F / 16) *
    cosSqAlpha *
    (4 + WGS84_F * (4 - 3 * cosSqAlpha));
  const bigL =
    lambda -
    (1 - c) *
      WGS84_F *
      sinAlpha *
      (sigma +
        c *
          sinSigma *
          (cos2SigmaM + c * cosSigma * (-1 + 2 * cos2SigmaM * cos2SigmaM)));

  return {
    lat: toDegrees(phi2),
    lon: normalizeLongitude(origin.lon + toDegrees(bigL)),
  };
}

export function coverageCircleCoordinates(point: MapPoint): [number, number][] {
  const coordinates: [number, number][] = [];
  for (let azimuth = 0; azimuth < 360; azimuth += 2) {
    const sample = directWgs84(point, azimuth, COVERAGE_RADIUS_M);
    coordinates.push([sample.lon, sample.lat]);
  }
  coordinates.push(coordinates[0] as [number, number]);
  return coordinates;
}

export function heatmapImageCorners(point: MapPoint): [number, number][] {
  const cornerDistance = COVERAGE_RADIUS_M * Math.sqrt(2);
  return [-45, 45, 135, -135].map((azimuth) => {
    const corner = directWgs84(point, azimuth, cornerDistance);
    return [corner.lon, corner.lat];
  });
}

export function maidenheadLocator(point: MapPoint): string {
  const longitude = Math.min(359.999_999, Math.max(0, point.lon + 180));
  const latitude = Math.min(179.999_999, Math.max(0, point.lat + 90));
  const fieldLon = Math.floor(longitude / 20);
  const fieldLat = Math.floor(latitude / 10);
  const squareLon = Math.floor((longitude % 20) / 2);
  const squareLat = Math.floor(latitude % 10);
  const subsquareLon = Math.floor((longitude % 2) * 12);
  const subsquareLat = Math.floor((latitude % 1) * 24);
  return `${String.fromCharCode(65 + fieldLon)}${String.fromCharCode(65 + fieldLat)}${squareLon}${squareLat}${String.fromCharCode(97 + subsquareLon)}${String.fromCharCode(97 + subsquareLat)}`;
}

export function graticuleGeoJson(): FeatureCollection<LineString> {
  const features: Feature<LineString>[] = [];
  for (let lon = 70; lon <= 140; lon += 5) {
    features.push({
      type: "Feature",
      properties: {},
      geometry: {
        type: "LineString",
        coordinates: [
          [lon, 10],
          [lon, 60],
        ],
      },
    });
  }
  for (let lat = 10; lat <= 60; lat += 5) {
    features.push({
      type: "Feature",
      properties: {},
      geometry: {
        type: "LineString",
        coordinates: [
          [70, lat],
          [140, lat],
        ],
      },
    });
  }
  return { type: "FeatureCollection", features };
}
