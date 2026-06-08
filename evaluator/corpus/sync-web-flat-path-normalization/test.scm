(define (bridge-marker? x) (eq? x '*bridge*))

(define (normalize-flat-path path)
  (let loop ((rest path) (indexes '()) (bridges '()) (tail '()))
    (cond ((null? rest)
           (list 'indexes (reverse indexes) 'bridges (reverse bridges) 'path (reverse tail)))
          ((integer? (car rest))
           (loop (cdr rest) (cons (car rest) indexes) bridges tail))
          ((and (bridge-marker? (car rest)) (pair? (cdr rest)))
           (loop (cddr rest) indexes (cons (cadr rest) bridges) tail))
          (else
           (loop (cdr rest) indexes bridges (cons (car rest) tail))))))

(list
  (normalize-flat-path '(*state* alice doc))
  (normalize-flat-path '(-1 *state* alice doc))
  (normalize-flat-path '(-1 *bridge* peer -2 *state* bob doc)))
