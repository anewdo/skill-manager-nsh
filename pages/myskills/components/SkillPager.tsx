import { Text, View } from "nsh";
import { Pagination } from "nsh/base";
import { paginationItemClass, type TextFn } from "../labels";

export interface SkillPagerProps {
  text: TextFn;
  summary: () => string;
  page: () => number;
  totalPages: () => number;
  onChange: (page: number) => void;
}

export function SkillPager(props: SkillPagerProps) {
  return (
    <View class="flex-row items-center justify-between border-t border-border bg-card px-6 py-3">
      <Text class="text-xs text-muted-foreground">{props.summary()}</Text>
      <If when={props.totalPages() > 1}>
        {() => (
          <Pagination
            page={props.page()}
            totalPages={props.totalPages()}
            onChange={(event) => props.onChange(event.page)}
            accessibilityLabel={props.text("myskills.pagination.label")}
            class="flex-row items-center gap-1"
            itemClass={paginationItemClass}
          />
        )}
      </If>
    </View>
  );
}
