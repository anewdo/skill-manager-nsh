import { Text, View } from "nsh";
import { Button } from "nsh/base";

export type ButtonVariant = "primary" | "secondary" | "ghost";

const buttonClass: Record<ButtonVariant, string> = {
  primary:
    "rounded-(--radius) bg-primary px-3 py-1.5 active:bg-primary/90 disabled:opacity-50",
  secondary:
    "rounded-(--radius) border border-border bg-card px-3 py-1.5 active:bg-accent disabled:opacity-50",
  ghost: "rounded-(--radius) px-2 py-1 active:bg-accent disabled:opacity-50",
};

const buttonLabelClass: Record<ButtonVariant, string> = {
  primary: "text-sm text-primary-foreground",
  secondary: "text-sm text-foreground",
  ghost: "text-sm text-muted-foreground",
};

export interface ActionButtonProps {
  label: string;
  onPress: () => void;
  variant?: ButtonVariant;
  disabled?: boolean;
  testID?: string;
}

export function ActionButton(props: ActionButtonProps) {
  const variant = props.variant ?? "secondary";
  return (
    <Button
      class={buttonClass[variant]}
      disabled={props.disabled === true}
      onPress={props.onPress}
      testID={props.testID}
    >
      <Text class={buttonLabelClass[variant]}>{props.label}</Text>
    </Button>
  );
}

export type BannerTone = "error" | "warning" | "success" | "info";

const bannerClass: Record<BannerTone, string> = {
  error:
    "flex-row items-center gap-3 rounded-(--radius) border border-destructive/40 bg-destructive/10 px-3 py-2",
  warning:
    "flex-row items-center gap-3 rounded-(--radius) border border-border bg-muted px-3 py-2",
  success:
    "flex-row items-center gap-3 rounded-(--radius) border border-border bg-secondary px-3 py-2",
  info: "flex-row items-center gap-3 rounded-(--radius) border border-border bg-muted px-3 py-2",
};

const bannerTextClass: Record<BannerTone, string> = {
  error: "flex-1 text-sm text-destructive",
  warning: "flex-1 text-sm text-foreground",
  success: "flex-1 text-sm text-foreground",
  info: "flex-1 text-sm text-muted-foreground",
};

export interface BannerProps {
  tone: BannerTone;
  message: string;
  actionLabel?: string;
  onAction?: () => void;
  testID?: string;
}

export function Banner(props: BannerProps) {
  return (
    <View class={bannerClass[props.tone]} testID={props.testID}>
      <Text class={bannerTextClass[props.tone]}>{props.message}</Text>
      <If when={props.actionLabel !== undefined && props.onAction !== undefined}>
        {() => (
          <ActionButton
            label={props.actionLabel ?? ""}
            variant="ghost"
            onPress={() => props.onAction?.()}
          />
        )}
      </If>
    </View>
  );
}

export function Badge(props: { label: string }) {
  return (
    <View class="rounded-full bg-secondary px-2 py-0.5">
      <Text class="text-xs text-secondary-foreground">{props.label}</Text>
    </View>
  );
}
