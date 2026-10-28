import { tagChipAttrs, tagColorOf } from '../utils/tagColors'
import { projectOf } from '../utils/text'
import { tagPropertiesFor } from '../utils/tagPropertiesStore'

/**
 * Attributes for a `.tag` chip of `taskId` (DEV-20): the configured colour of
 * the tag in the task's project, once that project's config has loaded.
 * Reactive: chips recolour when the project's config changes.
 */
export function tagAttrs(taskId: string | null | undefined, tag: string) {
    return tagChipAttrs(tagColorOf(tagPropertiesFor(projectOf(taskId ?? '')), tag))
}

/** The same for a known project prefix (task panel, create mode). */
export function tagAttrsForProject(prefix: string | null | undefined, tag: string) {
    return tagChipAttrs(tagColorOf(tagPropertiesFor(prefix), tag))
}
