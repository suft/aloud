import { toast } from "svelte-sonner";
import { errorMessage } from "./api";

/** Awaits `promise`; a rejection becomes an error toast and yields `undefined`. */
export async function attempt<T>(promise: Promise<T>): Promise<T | undefined> {
  try {
    return await promise;
  } catch (e) {
    toast.error(errorMessage(e));
    return undefined;
  }
}
