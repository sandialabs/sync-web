(lambda (assertions-src standard-src tree-src)

  (eval assertions-src)

  (define standard-class ((eval standard-src) 'class))
  (define load-node (eval '(lambda (node) (sync-eval node))))
  (define combine-node
    (eval '(lambda (trusted node)
             (sync-eval (sync-cons (sync-car trusted) (sync-cdr node))))))
  (define (compile class)
    (let ((method (caddr standard-class)))
      ((eval `(lambda* ,(cddadr method) ,@(cddr method))) class)))
  (define portable (load-node (compile standard-class)))
  (define* (standard (operation #f))
    (case operation
      ((make) compile)
      ((init) (lambda (class . args)
                (let ((object (load-node (compile class))))
                  (if (member '*init* (object '*api*))
                      (apply (object '*init*) args))
                  (object))))
      (else (if operation (portable operation) (portable)))))

  (define (caught thunk)
    (catch #t thunk (lambda args (list 'error (car args)))))

  (define tree-1 (sync-eval (assert ((standard 'init) tree-src) sync-node?)))
  (define tree-2 (sync-eval (assert ((standard 'init) tree-src) sync-node?)))
  (define tree-3 (sync-eval (assert ((standard 'init) tree-src) sync-node?)))

  ;; Direct directory traversal must consume the same MSB-first hash bits as
  ;; proof-producing and mutating directory operations.
  (assert
   (let key-loop ((i 0))
     (or (= i 64)
         (let* ((key ((tree-1 '~key->bytes) `(key ,i ,(* i i))))
                (hash (sync-hash key))
                (bits ((tree-1 '~key-bits) key)))
           (and (= (length bits) 256)
                (let bit-loop ((depth 0) (bits bits))
                  (or (= depth 256)
                      (and (= (car bits)
                              (logand (ash (hash (ash depth -3))
                                           (- (modulo depth 8) 7)) 1))
                           (bit-loop (+ depth 1) (cdr bits)))))
                (key-loop (+ i 1))))))
   #t)

  (assert ((tree-1 'set!) '(a b) 2) #t)
  (assert ((tree-1 'set!) '(a c d) 4) #t)
  (assert ((tree-1 'set!) '(a c* d) 4) #t)
  (assert ((tree-1 'get) '(a c d)) 4)
  (assert ((tree-1 'set!) '(a e f) 9) #t)
  (assert ((tree-1 'set!) '(a e g) 10) #t)

  ;; Tree keys preserve integer, symbol, and Scheme-string identity.
  (let ((numeric-symbol '|123|))
    (assert ((tree-1 'set!) '(typed 123) 'integer-value) #t)
    (assert ((tree-1 'set!) (list 'typed numeric-symbol) 'symbol-value) #t)
    (assert ((tree-1 'set!) '(typed "123") 'string-value) #t)
    (assert ((tree-1 'get) '(typed 123)) 'integer-value)
    (assert ((tree-1 'get) (list 'typed numeric-symbol)) 'symbol-value)
    (assert ((tree-1 'get) '(typed "123")) 'string-value)
    (let ((entries (cadr ((tree-1 'get) '(typed)))))
      (assert (not (not (member '(123 value) entries))) #t)
      (assert (not (not (member (list numeric-symbol 'value) entries))) #t)
      (assert (not (not (member '("123" value) entries))) #t)))

  ;; Reader-opaque symbols cannot become durable Tree keys. Every public path
  ;; operation uses the same admission, while non-symbol keys remain valid.
  (let ((opaque (string->symbol "."))
        (digest (sync-digest (tree-1))))
    (assert (caught (lambda () ((tree-1 'set!) (list opaque) 'value)))
            (lambda (result) (equal? result '(error read-error))))
    (assert (equal? digest (sync-digest (tree-1))) #t)
    (assert ((tree-1 'get) '())
            (lambda (result) (and (pair? result) (eq? (car result) 'directory))))
    (assert (caught (lambda () ((tree-1 'get) (list opaque))))
            (lambda (result) (equal? result '(error read-error))))
    (assert ((tree-1 'set!) '("." 1) 'non-symbol-value) #t)
    (assert ((tree-1 'get) '("." 1)) 'non-symbol-value))

  (assert ((tree-1 'set!) '(a e f) '(nothing)) #t)
  (assert ((tree-1 'set!) '(a e) '(nothing)) #t)

  (assert ((tree-1 'get) '(a)) '(directory ((c directory) (c* directory) (b value)) #t))
  (assert ((tree-1 'get) '(a b)) 2)
  (assert ((tree-1 'get) '(a c d)) 4)
  (assert ((tree-1 'get) '(a c* d)) 4)

  (assert ((tree-1 'equal?) '(a b) '(a c)) #f)
  (assert ((tree-1 'equal?) '(a c) '(a c*)) #t)

  (assert ((tree-1 'copy!) '(a) '(a*)) #t)
  (assert ((tree-1 'get) '(a* c d)) 4)

  ;; (assert (catch #t
  ;;           (lambda () ((tree-1 'set!) '(a fn) (lambda (x) x)))
  ;;           (lambda args args))
  ;;         (lambda (x) (and (list? x) (eq? (car x) 'value-error))))

  ;; (assert (catch #t
  ;;           (lambda () ((tree-1 'set!) '(a mac) (macro (x) x)))
  ;;           (lambda args args))
  ;;         (lambda (x) (and (list? x) (eq? (car x) 'value-error))))

  (assert ((tree-1 'get) '(a)) '(directory ((c directory) (c* directory) (b value)) #t))
  (assert ((tree-1 'slice!) '(a b)) #t)
  (assert ((tree-1 'get) '(a b)) 2)
  (assert ((tree-1 'get) '(a)) '(directory ((b value)) #f))

  (assert ((tree-1 'set!) '(b a c d) 4) #t)
  (assert ((tree-1 'set!) '(b d d) 2) #t)
  (assert ((tree-1 'set!) '(b d e) 5) #t)
  (assert ((tree-1 'set!) '(b n c d) 1) #t)
  (assert ((tree-1 'copy!) '(b) '(b*)) #t)
  (assert ((tree-1 'prune!) '(b d d) #t) #t)
  (assert ((tree-1 'prune!) '(b d e)) #t)
  (assert ((tree-1 'get) '(b d)) '(directory ((d unknown)) #f))
  (assert ((tree-1 'get) '(b d d)) '(unknown))
  (assert ((tree-1 'get) '(b n c d)) 1)

  (assert ((tree-1 'equal?) '(b) '(b*)) #f)
  (assert ((tree-1 'equivalent?) '(b) '(b*)) #t)

  (assert ((tree-1 'valid?)) #t)

  (assert ((tree-2 'set!) '(a b) 2) #t)
  (assert ((tree-2 'set!) '(a* b) 4) #t)
  (assert ((tree-2 'prune!) '(a b)) #t)

  (assert ((tree-3 'set!) '(a b) 2) #t)
  (assert ((tree-3 'set!) '(a* b) 4) #t)
  (assert ((tree-3 'prune!) '(a* b)) #t)

  (assert ((tree-2 'merge!) (tree-3)) #t)
  (assert ((tree-2 'get) '(a b)) 2)
  (assert ((tree-2 'get) '(a* b)) 4)

  ;; Ordered copy batches capture every raw source before mutation. Duplicate
  ;; targets are last-write-wins, while overlapping sources retain snapshot data.
  (let ((tree (sync-eval ((standard 'init) tree-src))))
    ((tree 'set!) '(source-1) 'one)
    ((tree 'set!) '(source-2) 'two)
    ((tree 'set!) '(directory child) 'child)
    ((tree 'set!) '(delete-me) 'old)
    (assert ((tree 'copy-batch!)
             '((source-1) (source-2) (directory) (missing))
             '((duplicate) (duplicate) (directory nested) (delete-me))) #t)
    (assert ((tree 'get) '(duplicate)) 'two)
    (assert ((tree 'get) '(directory nested child)) 'child)
    (assert ((tree 'get) '(delete-me)) '(nothing))
    (assert ((tree 'copy-batch!) '() '()) #t)
    (assert ((tree 'copy-batch!) (make-list 1024 '(source-1))
             (make-list 1024 '(maximum))) #t)
    (assert ((tree 'get) '(maximum)) 'one)
    (let ((digest (sync-digest (tree))))
      (assert (caught
               (lambda ()
                 ((tree 'copy-batch!) (make-list 1025 '(source-1))
                  (make-list 1025 '(too-many)))))
              (lambda (result) (equal? result '(error argument-error))))
      (assert (equal? digest (sync-digest (tree))) #t)))

  ;; A directory whose immediate map is known may still contain a deeper stub.
  ;; Batch validation rejects it recursively before changing the target.
  (let* ((full (sync-eval ((standard 'init) tree-src)))
         (_ ((full 'set!) '(dir a x) 'x))
         (_ ((full 'set!) '(dir a y) 'y))
         (_ ((full 'set!) '(target) 'old))
         (source-view (sync-eval (full)))
         (target-view (sync-eval (full))))
    ((source-view 'slice!) '(dir a x))
    ((target-view 'slice!) '(target))
    ((source-view 'merge!) (target-view))
    (assert (caddr ((source-view 'get) '(dir))) #t)
    (assert ((source-view 'get) '(dir a y)) '(unknown))
    (let ((digest (sync-digest (source-view))))
      (assert (caught
               (lambda () ((source-view 'copy-batch!) '((dir)) '((target)))))
              (lambda (result) (equal? result '(error availability-error))))
      (assert (equal? digest (sync-digest (source-view))) #t)
      (assert ((source-view 'get) '(target)) 'old))
    ;; The preexisting scalar Tree copy! remains unchanged outside P2.
    (assert ((source-view 'copy!) '(dir) '(legacy-copy)) #t)
    (assert ((source-view 'get) '(legacy-copy a x)) 'x)
    (assert ((source-view 'get) '(legacy-copy a y)) '(unknown)))

  ;; Batch validation happens before any write.
  (let ((tree (sync-eval ((standard 'init) tree-src))))
    (assert (caught (lambda () ((tree 'set-batch!) '((x) (y)) '(1))))
            (lambda (result) (equal? result '(error argument-error))))
    (assert ((tree 'get) '(x)) '(nothing))
    (assert ((tree 'get) '(y)) '(nothing)))

  ;; The empty path denotes the map itself and cannot become a scalar.
  (let ((tree (sync-eval ((standard 'init) tree-src))))
    ((tree 'set!) '(source child) 'value)
    (let ((digest (sync-digest (tree))))
      (assert (caught (lambda () ((tree 'set!) '() 'scalar)))
              (lambda (result) (equal? result '(error path-error))))
      (assert (caught
               (lambda ()
                 ((tree 'set!) '() (sync-cons (sync-null) (sync-null)))))
              (lambda (result) (equal? result '(error path-error))))
      (assert (caught (lambda () ((tree 'copy!) '(source child) '())))
              (lambda (result) (equal? result '(error path-error))))
      (assert (equal? digest (sync-digest (tree))) #t))
    ;; A directory subtree can still replace the complete map.
    (assert ((tree 'copy!) '(source) '()) #t)
    (assert ((tree 'get) '(child)) 'value)
    (assert ((tree 'get) '(source)) '(nothing))
    (assert ((tree 'set!) '() '(nothing)) #t)
    (assert ((tree 'get) '()) '(nothing))
    (assert ((tree 'valid?)) #t))

  ;; Mutation semantics do not depend on whether a scalar is materialized.
  (let ((partial-delete (sync-eval ((standard 'init) tree-src)))
        (partial-write (sync-eval ((standard 'init) tree-src))))
    (for-each (lambda (tree)
                ((tree 'set!) '(a known) 'known-value)
                ((tree 'set!) '(a hidden) 'hidden-value)
                ((tree 'slice!) '(a known)))
              (list partial-delete partial-write))
    (let ((delete-digest (sync-digest (partial-delete)))
          (write-digest (sync-digest (partial-write))))
      (assert (caught
               (lambda ()
                 ((partial-delete 'set!) '(a hidden child) '(nothing))))
              (lambda (result) (equal? result '(error availability-error))))
      (assert (caught
               (lambda ()
                 ((partial-write 'set!) '(a hidden child) 'replacement)))
              (lambda (result) (equal? result '(error availability-error))))
      (assert (equal? delete-digest (sync-digest (partial-delete))) #t)
      (assert (equal? write-digest (sync-digest (partial-write))) #t)
      (assert ((partial-delete 'get) '(a hidden)) '(unknown))
      (assert ((partial-write 'get) '(a hidden)) '(unknown))))

  ;; Missing-path slices are digest-preserving non-membership proofs.
  (let ((top (sync-eval ((standard 'init) tree-src)))
        (nested (sync-eval ((standard 'init) tree-src))))
    ((top 'set!) '(present) 'value)
    ((nested 'set!) '(dir present) 'value)
    (let ((top-digest (sync-digest (top)))
          (nested-digest (sync-digest (nested))))
      (assert ((top 'slice!) '(missing)) #t)
      (assert ((nested 'slice!) '(dir missing)) #t)
      (assert (equal? top-digest (sync-digest (top))) #t)
      (assert (equal? nested-digest (sync-digest (nested))) #t)
      (assert ((top 'get) '(missing)) '(nothing))
      (assert ((nested 'get) '(dir missing)) '(nothing))
      (assert (assoc 'missing (cadr ((top 'get) '()))) #f)
      (assert (assoc 'missing (cadr ((nested 'get) '(dir)))) #f)))

  ;; Copying `(nothing)` has the same deletion semantics as set!.
  (let ((tree (sync-eval ((standard 'init) tree-src))))
    ((tree 'set!) '(target) 'old)
    (assert ((tree 'copy!) '(missing) '(target)) #t)
    (assert ((tree 'get) '(target)) '(nothing))
    (assert ((tree 'get) '()) '(nothing)))

  ;; Equivalent empty trees merge as a successful no-op.
  (let ((a (sync-eval ((standard 'init) tree-src)))
        (b (sync-eval ((standard 'init) tree-src))))
    (assert ((a 'merge!) (b)) #t))

  ;; Complementary proofs of an opaque structured value merge by raw digest
  ;; without evaluating the stored value as an object.
  (let* ((full (sync-cons #u(1 2 3) #u(4 5 6)))
         (left (sync-cons (sync-car full) (sync-cut (sync-cdr full))))
         (right (sync-cons (sync-cut (sync-car full)) (sync-cdr full)))
         (tree-left (sync-eval ((standard 'init) tree-src)))
         (tree-right (sync-eval ((standard 'init) tree-src))))
    ((tree-left 'set!) '(opaque) left)
    ((tree-right 'set!) '(opaque) right)
    (assert (equal? (sync-digest (tree-left))
                    (sync-digest (tree-right))) #t)
    (assert ((tree-left 'merge!) (tree-right)) #t)
    (let ((merged ((tree-left 'get) '(opaque))))
      (assert (sync-car merged) #u(1 2 3))
      (assert (sync-cdr merged) #u(4 5 6))))

  (append "Success (" (object->string asserted) " checks)"))
