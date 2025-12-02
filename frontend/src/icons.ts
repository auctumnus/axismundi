import { addIcon } from '@iconify/react/dist/offline'
import iconData from '@iconify-json/material-symbols/icons.json'

// List of icons we use (Material Icons naming with underscores)
const usedIcons = [
  'format_bold',
  'format_italic',
  'format_underlined',
  'code',
  'looks_one',
  'looks_two',
  'format_quote',
  'format_list_numbered',
  'format_list_bulleted',
  'format_align_left',
  'format_align_center',
  'format_align_right',
  'format_align_justify',
]

// Convert Material Icons names (underscores) to Iconify names (dashes)
// and add them to the offline bundle
for (const iconName of usedIcons) {
  const iconifyName = iconName.replace(/_/g, '-')
  const fullIconName = `material-symbols:${iconifyName}`

  // Find the icon in the JSON data
  const iconDataEntry = (iconData.icons as Record<string, any>)[iconifyName]
  if (iconDataEntry) {
    addIcon(fullIconName, {
      ...iconDataEntry,
      width: iconData.width,
      height: iconData.height,
    })
  } else {
    console.warn(`Icon ${iconifyName} not found in material-symbols`)
  }
}
