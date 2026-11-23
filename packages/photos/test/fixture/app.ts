// The suite writes the id `exact new` gave the app. The list is read only
// once the suite asks, since the first read while `prompt` asks (D7).
import { photoAssets } from '@exact/photos';

export const appId = '__APP_ID__';
export const grants = 'device.photos purpose.read\ndevice.photos-add purpose.add\nfs.read app:/tmp\nfs.write app:/tmp';

const sources: Record<string, (args: any) => Promise<unknown>> = {
  assets: async ([read]: [boolean]) => (read ? await photoAssets({ limit: 24 }) : { assets: [], cursor: null }),
  // Every page, each asked with the cursor of the one before, the screenshots
  // alone when `shots`: how many, how many distinct ids, whether newest first
  // throughout, whether each is marked a screenshot, and the first id. The
  // filter's word is given twice, as the module takes it once.
  everything: async ([size, shots]: [number, boolean]) => {
    const ids = new Set<string>();
    let pages = 0, count = 0, ordered = true, marked = true, previous = Infinity, first = '', cursor: string | undefined;
    while (size > 0) {
      const page = await photoAssets({ limit: size, cursor, subtypes: shots ? ['screenshot', 'screenshot'] : [] });
      pages++;
      for (const a of page.assets) {
        first ||= a.id;
        ids.add(a.id);
        count++;
        const date = a.creationDate ?? -Infinity;
        if (date > previous) ordered = false;
        if (!a.apple?.mediaSubtypes.includes('photoScreenshot')) marked = false;
        previous = date;
      }
      if (!page.cursor) break;
      cursor = page.cursor;
    }
    return { pages, count, distinct: ids.size, ordered, marked, first };
  },
};

export const answer = ((source: string, args: unknown) => sources[source](args)) as any;
