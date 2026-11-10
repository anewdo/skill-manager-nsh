import { Text, View } from "nsh";
import { Button } from "nsh/base";
import { Icon, type IconColor, type IconName } from "./icon";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";

const buttonClass: Record<ButtonVariant, string> = {
  primary:
    "rounded-(--radius) bg-primary px-3 py-1.5 active:bg-primary/90 disabled:opacity-50",
  secondary:
    "rounded-(--radius) border border-border bg-card px-3 py-1.5 active:bg-accent disabled:opacity-50",
  ghost: "rounded-(--radius) px-2 py-1 active:bg-accent disabled:opacity-50",
  danger:
    "rounded-(--radius) border border-destructive/40 bg-destructive/10 px-3 py-1.5 active:bg-destructive/20 disabled:opacity-50",
};

const buttonLabelClass: Record<ButtonVariant, string> = {
  primary: "text-sm text-primary-foreground",
  secondary: "text-sm text-foreground",
  ghost: "text-sm text-muted-foreground",
  danger: "text-sm text-destructive",
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

export type IconButtonTone = "default" | "danger";

const iconButtonClass: Record<IconButtonTone, string> = {
  default:
    "h-7 w-7 items-center justify-center rounded-(--radius) active:bg-accent disabled:opacity-40",
  danger:
    "h-7 w-7 items-center justify-center rounded-(--radius) active:bg-destructive/10 disabled:opacity-40",
};

const iconButtonIconColor: Record<IconButtonTone, IconColor> = {
  default: "foreground",
  danger: "destructive",
};

export interface IconButtonProps {
  icon: IconName;
  label: string;
  onPress: () => void;
  tone?: IconButtonTone;
  disabled?: boolean;
  count?: number;
  testID?: string;
}

export function IconButton(props: IconButtonProps) {
  const tone = props.tone ?? "default";
  return (
    <Button
      class={iconButtonClass[tone]}
      disabled={props.disabled === true}
      onPress={props.onPress}
      accessibilityLabel={props.label}
      testID={props.testID}
    >
      <View class="flex-row items-center justify-center gap-1">
        <Icon
          name={props.icon}
          color={iconButtonIconColor[tone]}
          accessibilityLabel={props.label}
        />
        <If when={props.count !== undefined}>
          {() => (
            <Text class={tone === "danger" ? "text-xs text-destructive" : "text-xs text-foreground"}>
              {String(props.count ?? 0)}
            </Text>
          )}
        </If>
      </View>
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

export type BadgeTone = "default" | "warning" | "danger" | "success" | "info";

const badgeClass: Record<BadgeTone, string> = {
  default: "bg-secondary",
  warning: "bg-muted",
  danger: "bg-destructive/10",
  success: "bg-secondary",
  info: "bg-muted",
};

const badgeTextClass: Record<BadgeTone, string> = {
  default: "text-secondary-foreground",
  warning: "text-foreground",
  danger: "text-destructive",
  success: "text-foreground",
  info: "text-muted-foreground",
};

export function Badge(props: { label: string; tone?: BadgeTone }) {
  const tone = props.tone ?? "default";
  return (
    <View class={`rounded-full px-2 py-0.5 ${badgeClass[tone]}`}>
      <Text class={`text-xs ${badgeTextClass[tone]}`}>{props.label}</Text>
    </View>
  );
}
