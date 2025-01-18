import { default as React } from 'react';
import clsx from 'clsx';

export type IButtonProps = React.ButtonHTMLAttributes<HTMLButtonElement>;

export const Button: React.FC<IButtonProps> = ({
  children,
  className,
  disabled,
  ...props
}) => (
  <button
    className={clsx(
      'bg-slate-600 dark:bg-slate-400 hover:bg-gray-400 hover:text-gray-700 text-gray-100 dark:text-gray-800 font-bold py-2 px-4',
      { 'opacity-60 cursor-not-allowed': disabled },
      className,
    )}
    {...props}
  >
    {children}
  </button>
);
