import { defineCollection, z } from 'astro:content';
import { docsLoader } from '@astrojs/starlight/loaders';
import { docsSchema } from '@astrojs/starlight/schema';

export const collections = {
	docs: defineCollection({
		loader: docsLoader(),
		schema: docsSchema({
			extend: z.object({
				publishDate: z.coerce.date().optional(),
				summary: z.string().optional(),
				author: z.string().optional(),
			}),
		}),
	}),
};
