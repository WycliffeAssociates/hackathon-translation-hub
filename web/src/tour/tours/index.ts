import type { Tour } from "../machine";
import { demoTour } from "./demo";

/** Every tour, each in its own file. Started by `?tour=<id>`. */
export const TOURS: readonly Tour[] = [demoTour];

export const tourById = (id: string): Tour | undefined => TOURS.find((tour) => tour.id === id);
