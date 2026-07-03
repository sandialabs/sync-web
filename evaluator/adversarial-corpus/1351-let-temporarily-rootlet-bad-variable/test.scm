(let ((x 1)) (let-temporarily (((rootlet) 'car) cdr) (car (list 1 2))))
