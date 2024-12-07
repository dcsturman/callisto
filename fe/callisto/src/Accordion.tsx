import { useState, ReactNode } from 'react';
import { FaChevronUp } from "react-icons/fa";

export function Accordion({ className, id, title, initialOpen, children } : { className?: string, id?: string, title: string, initialOpen?: boolean, children?: ReactNode }) {
  const [isOpen, setIsOpen] = useState(initialOpen === undefined || initialOpen);

  const toggle = () => {
    setIsOpen(!isOpen);
  };

  return (
    <div className={"accordion " + className} id={id}>
      <span className="accordion-header" onClick={toggle}>{title}<FaChevronUp className={`chevron ${isOpen ? 'open' : 'closed'}`}/></span>
      {isOpen && (
        <div className={"accordion-content " + className+"-header"}>{children}</div>
      )}
    </div>
  );
}
export default Accordion;