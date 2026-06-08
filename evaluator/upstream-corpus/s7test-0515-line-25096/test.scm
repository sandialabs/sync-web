;; Imported from upstream s7test.scm line 25096.
;; Original form:
;; (test (object->string (let ((c 3)) (define (ex1 a b) (+ a c b))) :readable) "(let ((c 3)) (lambda (a b) (+ a c b)))")

(define (upstream-safe thunk)
  (catch #t
    (lambda () (list 'value (thunk)))
    (lambda args (list 'error args))))

(let* ((actual (upstream-safe (lambda () (object->string (let ((c 3)) (define (ex1 a b) (+ a c b))) :readable))))
       (expected (upstream-safe (lambda () "(let ((c 3)) (lambda (a b) (+ a c b)))")))
       (ok? (equal? actual expected)))
  (list 'upstream-test 25096 actual expected ok?))
