import { useState, ReactNode } from 'react';
import { FaChevronUp } from "react-icons/fa";

export function Accordion(args: { title: string, initialOpen?: boolean, children?: ReactNode }) {
  const [isOpen, setIsOpen] = useState(args.initialOpen === undefined || args.initialOpen);

  const toggle = () => {
    setIsOpen(!isOpen);
  };

  return (
    <div className="accordion">
      <span className="accordion-header" onClick={toggle}>{args.title}<FaChevronUp className={`chevron ${isOpen ? 'open' : 'closed'}`}/></span>
      {isOpen && (
        <div className="accordion-content">{args.children}</div>
      )}
    </div>
  );
}
export default Accordion;