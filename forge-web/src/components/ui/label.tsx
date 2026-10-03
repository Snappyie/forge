import { cn } from "cn"

/**
 * Accessible form label.
 *
 * `@base-ui/react` ships no `label` primitive, so this wraps a native
 * `<label>`. Pass `htmlFor` to associate with a control; that association is
 * what lets clicking the label focus the input and gives screen readers an
 * accessible name.
 */
function Label({
  className,
  ...props
}: React.ComponentProps<"label">) {
  return (
    <label
      data-slot="label"
      className={cn(
        "flex items-center gap-2 text-sm leading-none font-medium select-none",
        "group-data-disabled/field:opacity-50",
        "peer-disabled:cursor-not-allowed peer-disabled:opacity-50",
        className
      )}
      {...props}
    />
  )
}

export { Label }