import { displayTagColor, tagChipAttrs } from '../utils/tagColors'
import { tagPropertiesFor } from '../utils/tagPropertiesStore'
import { projectOf } from '../utils/text'

/**
 * Attributes for a `.tag` chip of `taskId` (DEV-20): the configured colour of
 * the tag in the task's project, or its automatic colour (DEV-229) unless
 * that is turned off. Chips stay plain until the project's config has loaded,
 * so a configured colour never flashes in after an automatic one.
 * Reactive: chips recolour when the config or the preference changes.
 */
export function tagAttrs(taskId: string | null | undefined, tag: string) {
    return tagAttrsForProject(projectOf(taskId ?? ''), tag)
}

/** The same for a known project prefix (task panel, create mode). */
export function tagAttrsForProject(prefix: string | null | undefined, tag: string) {
    const properties = tagPropertiesFor(prefix)
    return properties ? tagChipAttrs(displayTagColor(properties, tag)) : {}
}
