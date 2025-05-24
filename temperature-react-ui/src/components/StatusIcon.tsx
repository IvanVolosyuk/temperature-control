import React from 'react';

type StatusIconProps = {
  type: 'thermometer' | 'power';
  className?: string;
};

const StatusIcon: React.FC<StatusIconProps> = ({ type, className = '' }) => {
  if (type === 'thermometer') {
    return (
      <svg
        className={`w-6 h-6 mr-2 ${className}`}
        xmlns="http://www.w3.org/2000/svg"
        width="24"
        height="24"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <path d="M14 4v10.54a4 4 0 1 1-4 0V4a2 2 0 0 1 4 0Z" />
      </svg>
    );
  }

  if (type === 'power') {
    return (
      <svg
        className={`w-6 h-6 mr-2 ${className}`}
        xmlns="http://www.w3.org/2000/svg"
        width="24"
        height="24"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <path d="M12 2v10" />
        <path d="M18.4 6.6a9 9 0 1 1-12.77.04" />
      </svg>
    );
  }

  return null;
};

export default StatusIcon; 