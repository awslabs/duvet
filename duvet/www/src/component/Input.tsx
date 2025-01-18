import { default as React } from 'react';

export type ISelectProps = React.SelectHTMLAttributes<HTMLSelectElement>;

export const Select: React.FC<ISelectProps> = ({ children, ...props }) => (
  <div className="inline-block relative">
    <select
      className="overflow-y-auto block appearance-none w-full bg-white dark:bg-neutral-700 border border-gray-400 hover:border-gray-500 px-4 py-2 pr-8 rounded shadow leading-tight focus:outline-none focus:shadow-outline"
      {...props}
    >
      {children}
    </select>

    {!props.multiple ? (
      <div className="pointer-events-none absolute inset-y-0 right-0 flex items-center px-2 text-gray-700 dark:text-white">
        <svg
          className="fill-current h-4 w-4"
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 20 20"
        >
          <path d="M9.293 12.95l.707.707L15.657 8l-1.414-1.414L10 10.828 5.757 6.586 4.343 8z" />
        </svg>
      </div>
    ) : null}
  </div>
);

export interface IMultiSelectProps extends ISelectProps {
  value: Set<string>;
  onChange: (value: Set<string>) => void;
}

export const MultiSelect: React.FC<IMultiSelectProps> = ({
  children,
  value,
  onChange,
  ...props
}) => {
  function mapValue(evt) {
    const selectedOptions = evt.target.selectedOptions;
    const newValue = new Set<string>();
    for (let index = 0; index < selectedOptions.length; index++) {
      const option = selectedOptions[index];
      if (option.value == '') {
        return onChange(new Set());
      }
      newValue.add(option.value);
    }
    onChange(newValue);
  }

  return (
    <Select multiple value={Array.from(value)} onChange={mapValue} {...props}>
      <option value="">-</option>
      {children}
    </Select>
  );
};

export interface IFlagProps {
  enabled: string;
  disabled: string;
  wildcard: string;
  value: boolean | null;
  onChange: (value: boolean | null) => void;
}

export const Flag: React.FC<IFlagProps> = ({
  enabled = 'True',
  disabled = 'False',
  wildcard = '-',
  value,
  onChange,
}) => {
  const mappedValue = value === null ? wildcard : value ? enabled : disabled;
  return (
    <Select
      value={mappedValue}
      onChange={(evt) => {
        const value = evt.target.value;
        if (value == enabled) return onChange(true);
        if (value == disabled) return onChange(false);
        onChange(null);
      }}
    >
      <option value={wildcard}>{wildcard}</option>
      <option value={enabled}>{enabled}</option>
      <option value={disabled}>{disabled}</option>
    </Select>
  );
};
