(lambda (assertions-src standard-src authorization-src)

  (eval assertions-src)

  (define module (eval standard-src))
  (define standard-class (module 'class))
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
      ((init) (lambda (class . args)
                (let ((object (load-node (compile class))))
                  (if (member '*init* (object '*api*))
                      (apply (object '*init*) args))
                  (object))))
      ((local) (lambda (class node)
                 (combine-node (compile class) node)))
      (else (if operation (portable operation) (portable)))))

  (define auth-node ((standard 'init) authorization-src))
  (define auth ((standard 'local) authorization-src auth-node))

  (define owner '(*state* alice))
  (define local-peer '(*state* bob))
  (define remote-peer '(journal-2 *state* bob))
  (define public '(*public*))
  (define admins '((*state* admin)))
  (define remote-context '((latest-index 10) (authentication-index 10)))

  (define (request function path)
    `((function ,function) (arguments ((path ,path)))))

  (define (trace-request index path)
    `((function trace) (arguments ((index ,index) (path ,path)))))

  ;; Interface-composition helpers around the fixed Authorization protocol.
  (define (authorize! user rule latest-index)
    ((auth 'authorize!)
     `((user ,user) (rule ,rule) (latest-index ,latest-index))))

  (define (deauthorize! user rule)
    ((auth 'deauthorize!) `((user ,user) (rule ,rule))))

  (define* (authorize? admins principal request latest-index (context '()))
    (let* ((operation (cadr (assoc 'function request)))
           (args (cadr (assoc 'arguments request)))
           (remote? (assoc 'authentication-index context))
           (context `((latest-index ,latest-index)
                      ,@(if remote?
                            `((authentication-index ,(cadr remote?))) '()))))
      (or (and (not remote?) (not (not (member principal admins))))
          (and (memq operation '(authorizations authorize! deauthorize!))
               (assoc 'user args)
               (equal? principal (cadr (assoc 'user args))))
          (and (eq? operation 'set-batch!)
               (let ((paths (cadr (assoc 'paths args))))
                 (and (pair? paths)
                      (let loop ((paths paths))
                        (or (null? paths)
                            (and (eq? ((auth 'authorized?)
                                       principal context (car paths) 'set!)
                                      'direct)
                                 (loop (cdr paths))))))))
          (and (assoc 'path args)
               (let* ((path (cadr (assoc 'path args)))
                      (path
                       (if (and (eq? operation 'trace)
                                (assoc 'index args)
                                (or (null? path)
                                    (not (integer? (car path)))))
                           (cons (cadr (assoc 'index args)) path) path)))
                 (not (not ((auth 'authorized?)
                            principal context path operation))))))))

  (define* (directory-projection? admins principal request latest-index
                                  (context '()))
    (let* ((operation (cadr (assoc 'function request)))
           (args (cadr (assoc 'arguments request)))
           (path (cadr (assoc 'path args)))
           (context `((latest-index ,latest-index)
                      ,@(if (assoc 'authentication-index context)
                            `((authentication-index
                               ,(cadr (assoc 'authentication-index context))))
                            '()))))
      (eq? ((auth 'authorized?) principal context path operation)
           'ancestor)))

  (define base-rule
    `((principal ,local-peer)
      (path (docs))
      (get #t)
      (set! #f)
      (resolve #f)))

  (define reordered-rule
    `((resolve #f)
      (set! #f)
      (get #t)
      (path (docs))
      (principal ,local-peer)))

  (assert ((auth 'authorizations) owner) '())

  ;; Owner and admins bypass explicit rules.
  (assert (authorize? admins owner (request 'get '(*state* alice docs a)) 10) #t)
  (assert (authorize? admins owner (request 'set! '(*state* alice docs a)) 10) #t)
  (assert (authorize? admins '(*state* admin) (request 'set! '(*state* alice docs a)) 10) #t)
  ;; Cross-user access is denied by default.
  (assert (authorize? admins local-peer (request 'get '(*state* alice docs a)) 10) #f)

  ;; Shape-restricted bridge discovery is public protocol metadata inherited by
  ;; authenticated local and remote principals. It does not admit application
  ;; tails, mutation, or remote administrator authority.
  (for-each
   (lambda (entry)
     (let ((principal (car entry)) (context (cadr entry)))
       (assert (authorize? admins principal (request 'get '(*bridge*)) 10 context) #t)
       (assert (authorize? admins principal (request 'resolve '(10 *bridge*)) 10 context) #t)
       (assert (authorize? admins principal (trace-request 10 '(*bridge* peer)) 10 context) #t)
       (assert (authorize? admins principal
                (request 'get '(*bridge* peer *state* alice public)) 10 context)
               #f)
       (assert (authorize? admins principal (request 'set! '(*bridge*)) 10 context) #f)))
   `((,public ()) (,local-peer ()) (,remote-peer ,remote-context)))
  (assert (authorize? admins '(journal-2 *state* admin)
           (request 'get '(*bridge*)) 10 remote-context) #t)
  (assert (authorize? admins '(journal-2 *state* admin)
           (request 'set! '(*state* alice docs a)) 10 remote-context) #f)

  ;; authorize! normalizes and deduplicates rules.
  (assert (authorize! owner base-rule 10) #t)
  (assert (authorize! owner reordered-rule 10) #t)
  (assert ((auth 'authorizations) owner)
          `(((principal ,local-peer)
             (path (docs))
             (get #t)
             (set! #f)
             (resolve #f))))

  ;; Rules are recursive path-prefix grants and function-specific.
  (assert (authorize? admins local-peer (request 'get '(*state* alice docs a)) 10) #t)
  (assert (authorize? admins local-peer (request 'get '(*state* alice other a)) 10) #f)
  (assert (authorize? admins local-peer (request 'set! '(*state* alice docs a)) 10) #f)

  ;; deauthorize! removes a normalized equivalent rule.
  (assert (deauthorize! owner reordered-rule) #t)
  (assert ((auth 'authorizations) owner) '())
  (assert (authorize? admins local-peer (request 'get '(*state* alice docs a)) 10) #f)

  ;; call! is an Interface-admin operation, not an Authorization permission.
  ;; Owners and ordinary local principals cannot call, and new rules naming the
  ;; removed field fail closed instead of creating an inert apparent grant.
  (assert (authorize? admins owner (request 'call! '(*state* alice docs program)) 10) #f)
  (assert (authorize? admins '(*state* admin)
           (request 'call! '(*state* alice docs program)) 10) #t)
  (define call-rule
    `((principal ,local-peer) (path (docs))
      (get #t) (set! #f) (call! #t) (resolve #f)))
  (assert
   (catch #t
          (lambda () (authorize! owner call-rule 10))
          (lambda args (list 'error (car args))))
   (lambda (x) (equal? x '(error authorization-error))))
  (assert ((auth 'authorizations) owner) '())

  ;; A pre-release policy table that already contains call! cannot grant it.
  (set! (auth '(1))
        (expression->byte-vector `((,owner (,call-rule)))))
  (assert ((auth 'authorized?) local-peer '((latest-index 10))
           '(*state* alice docs program) 'call!) #f)
  (set! (auth '(1)) (expression->byte-vector '()))

  ;; Remote and public principals can be granted explicitly.
  (define remote-rule
    `((principal ,remote-peer)
      (key-index (0 -1))
      (path (shared))
      (get #t)
      (set! #t)
      (resolve #t)))
  (assert (authorize! owner remote-rule 10) #t)
  (assert (authorize? admins remote-peer
           (request 'get '(*state* alice shared x)) 10 remote-context) #t)
  (assert (authorize? admins remote-peer
           (request 'set! '(*state* alice shared x)) 10 remote-context) #t)
  (assert (authorize? admins remote-peer
           (request 'pin! '(*state* alice shared x)) 10 remote-context) #f)
  (assert (authorize? admins remote-peer
           (request 'unpin! '(*state* alice shared x)) 10 remote-context) #f)
  (assert (authorize? admins remote-peer
           (request 'call! '(*state* alice shared x)) 10 remote-context) #f)
  ;; A bridge principal in the admin list does not bypass scoped remote rules.
  (assert (authorize? (cons remote-peer admins) remote-peer
           (request 'get '(*state* alice private x)) 10
           '((latest-index 10) (authentication-index 10)))
          #f)
  (assert (deauthorize! owner remote-rule) #t)

  ;; Same-user federation is still exact-route authority: another username,
  ;; a looped route, or an ungranted longer route does not inherit it.
  (define same-user '(journal-2 *state* alice))
  (define different-user '(journal-2 *state* bob))
  (define looped-user '(journal-2 journal-1 journal-2 *state* alice))
  (define same-user-rule
    `((principal ,same-user) (key-index (0 -1)) (path (private))
      (get #t) (set! #t) (resolve #t)))
  (assert (authorize! owner same-user-rule 10) #t)
  (assert (authorize? admins same-user
           (request 'get '(*state* alice private key)) 10
           '((latest-index 10) (authentication-index 10))) #t)
  (assert (authorize? admins different-user
           (request 'get '(*state* alice private key)) 10
           '((latest-index 10) (authentication-index 10))) #f)
  (assert (authorize? admins looped-user
           (request 'get '(*state* alice private key)) 10
           '((latest-index 10) (authentication-index 10))) #f)

  (define public-rule
    `((principal ,public)
      (path (published))
      (get #f)
      (set! #f)
      (resolve #t)))
  (assert (authorize! owner public-rule 10) #t)
  (assert (authorize? admins public (trace-request 5 '(*state* alice published doc)) 10) #t)
  (assert (authorize? admins public (request 'get '(*state* alice published doc)) 10) #f)

  ;; Public grants apply to every principal. Ancestor traversal reveals ordinary
  ;; immediate child names, while opening each child is authorized independently.
  (define public-data-rule
    `((principal ,public)
      (path (data public))
      (get #t)
      (set! #f)
      (resolve #t)))
  (define private-data-rule
    `((principal ,remote-peer)
      (key-index (0 -1))
      (path (data journal-2))
      (get #t)
      (set! #t)
      (resolve #t)))
  (assert (authorize! owner public-data-rule 10) #t)
  (assert (authorize! owner private-data-rule 10) #t)
  (assert (authorize? admins remote-peer
           (request 'get '(*state* alice data public key-0)) 10
           '((latest-index 10) (authentication-index 10))) #t)
  (assert (authorize? admins local-peer
           (request 'get '(*state* alice data public key-0)) 10) #t)
  (assert (authorize? admins remote-peer
           (request 'set! '(*state* alice data public key-0)) 10
           '((latest-index 10) (authentication-index 10))) #f)
  (assert (authorize? admins local-peer
           (request 'get '(*state* alice data journal-2 key-0)) 10) #f)
  (assert (authorize? admins remote-peer
           (request 'get '(*state* alice data journal-3 key-0)) 10
           '((latest-index 10) (authentication-index 10))) #f)
  (assert (directory-projection? admins remote-peer
           (request 'get '(*state* alice)) 10
           '((latest-index 10) (authentication-index 10))) #t)
  (assert (directory-projection? admins remote-peer
           (request 'get '(*state* alice data)) 10
           '((latest-index 10) (authentication-index 10))) #t)
  (assert (directory-projection? admins local-peer
           (request 'get '(*state* alice data)) 10) #t)
  ;; The user namespace root is itself an ancestor: a local owner or a
  ;; principal with any descendant grant can discover ordinary user folder
  ;; names, while opening each folder remains independently authorized.
  (assert (directory-projection? admins owner
           (request 'get '(*state*)) 10) #t)
  (assert (directory-projection? admins remote-peer
           (request 'get '(*state*)) 10
           '((latest-index 10) (authentication-index 10))) #t)
  (assert (directory-projection? admins public
           (request 'resolve '(-1 *state*)) 10) #t)
  (let* ((node ((standard 'init) authorization-src))
         (empty-auth ((standard 'local) authorization-src node)))
    (assert ((empty-auth 'authorized?) '(*state* mallory)
             '((latest-index 10)) '(*state*) 'get)
            'ancestor)
    (assert ((empty-auth 'authorized?)
             '(journal-2 *state* mallory)
             '((latest-index 10) (authentication-index 10))
             '(*state*) 'get)
            #f))

  ;; Resolve #f, #t, and explicit ranges.
  (define ranged-rule
    `((principal ,local-peer)
      (path (history))
      (get #f)
      (set! #f)
      (resolve (5 -2))))
  (assert (authorize! owner ranged-rule 10) #t) ; -2 => 9 at latest index 10
  (assert (authorize? admins local-peer (request 'resolve '(5 *state* alice history doc)) 10) #t)
  (assert (authorize? admins local-peer (request 'resolve '(9 *state* alice history doc)) 10) #t)
  (assert (authorize? admins local-peer (request 'resolve '(10 *state* alice history doc)) 10) #f)
  (assert (authorize? admins local-peer (trace-request -2 '(*state* alice history doc)) 10) #t)

  ;; Resolve requests require an explicit index for non-owner principals.
  (assert (authorize? admins local-peer (request 'resolve '(*state* alice history doc)) 10) #f)

  ;; Shape-restricted bridge protocol metadata is inherited from public, while
  ;; application state tails remain subject to ordinary policy.
  (assert (authorize? admins local-peer (request 'resolve '(-1 *bridge* journal-0 -1 *state* alice docs a)) 10) #f)
  (assert (authorize? admins local-peer (request 'get '(-1 *bridge* journal-0 -1 *state* alice docs a)) 10) #f)
  (assert (authorize? admins local-peer (request 'trace '(-1 *bridge* journal-0)) 10) #t)
  (assert (authorize? admins local-peer (request 'trace '(-1 *bridge* journal-0 -1 *crypto* interface public-key)) 10) #t)
  ;; Retention remains origin-local, but a local owner may retain a verified
  ;; proof reached through bridge history for that same owner's terminal data.
  (assert (authorize? admins owner
           (request 'pin! '(-1 *bridge* journal-0 -1 *state* alice docs a)) 10) #t)
  (assert (authorize? admins owner
           (request 'unpin! '(-1 *bridge* journal-0 -1 *state* alice docs a)) 10) #t)
  (assert (authorize? admins local-peer (request 'pin! '(-1 *bridge* journal-0 -1 *state* alice docs a)) 10) #f)
  (assert (authorize? admins local-peer (request 'unpin! '(-1 *bridge* journal-0 -1 *state* alice docs a)) 10) #f)
  (assert (authorize? admins owner
           (request 'pin! '(-1 *bridge* journal-0 -1 *crypto* interface public-key)) 10) #f)
  (assert (authorize? admins local-peer (request 'set! '(-1 *bridge* journal-0 -1 *state* alice docs a)) 10) #f)

  ;; Rule shape, removed call grants, remote key windows, and ranges fail fast
  ;; during configuration.
  (assert (catch #t
                 (lambda ()
                   (authorize! owner
                    '((principal (journal-2 *state* no-window))
                      (path (x)) (get #t) (set! #f) (resolve #t))
                    10))
                 (lambda args (list 'error (car args))))
          (lambda (x) (equal? x '(error authorization-error))))
  (for-each
   (lambda (principal)
     (assert
      (catch #t
             (lambda ()
               (authorize! owner
                `((principal ,principal) (key-index (0 -1))
                  (path (x)) (get #t) (set! #f) (resolve #t))
                10))
             (lambda args (list 'error (car args))))
      (lambda (x) (equal? x '(error authorization-error)))))
   '((*bridge*) (*bridge* 7) (*bridge* peer *state*)
     (*bridge* peer unexpected tail)))
  ;; The same bare aliases are a valid concise multi-hop journal principal.
  (assert (authorize! owner
           '((principal (peer unexpected tail)) (key-index (0 -1))
             (path (multi)) (get #t) (set! #f) (resolve #t))
           10)
          #t)
  (assert
   (catch #t
          (lambda ()
            (authorize! owner
             '((principal (*state* mallory))
               (principal (*public*))
               (path (x)) (get #t) (set! #f) (resolve #f))
             10))
          (lambda args (list 'error (car args))))
   (lambda (x) (equal? x '(error authorization-error))))
  (assert
   (catch #t
          (lambda ()
            (authorize! owner
             '((principal (*state* mallory)) (path (x))
               (get #t) (set! #f) (resolve #f) (surprise #t))
             10))
          (lambda args (list 'error (car args))))
   (lambda (x) (equal? x '(error authorization-error))))
  (assert
   (catch #t
          (lambda ()
            (authorize! owner
             '((principal (*state* mallory)) (path (x))
               (get #t) (set! #f) (call! inherited) (resolve #f))
             10))
          (lambda args (list 'error (car args))))
   (lambda (x) (equal? x '(error authorization-error))))

  ;; Relative start + absolute end is invalid; other combinations are allowed if increasing.
  (assert (catch #t
                 (lambda () (authorize! owner
                             `((principal ,local-peer) (path (bad)) (get #f) (set! #f) (resolve (-5 9)))
                             10))
                 (lambda args (list 'error (car args))))
          (lambda (x) (eq? (car x) 'error)))
  (assert (catch #t
                 (lambda () (authorize! owner
                             `((principal ,local-peer) (path (bad)) (get #f) (set! #f) (resolve (9 5)))
                             10))
                 (lambda args (list 'error (car args))))
          (lambda (x) (eq? (car x) 'error)))
  (assert (authorize! owner
           `((principal ,local-peer) (path (recent)) (get #f) (set! #f) (resolve (-5 -1)))
           10)
          #t)
  (assert (authorize! owner
           `((principal ,local-peer) (path (current)) (get #f) (set! #f) (resolve (10 -1)))
           10)
          #t)
  (for-each
   (lambda (window)
     (assert
      (catch #t
             (lambda ()
               (authorize! owner
                `((principal ,remote-peer) (key-index ,window)
                  (path (bad-window)) (get #t) (set! #f) (resolve #t))
                10))
             (lambda args (list 'error (car args))))
      (lambda (x) (equal? x '(error authorization-error)))))
   '((10 5) (-1 -5)))

  ;; Policy mutation is owner-only for non-admins; admins bypass.
  (assert (authorize? admins owner `((function authorize!) (arguments ((user ,owner)))) 10) #t)
  (assert (authorize? admins local-peer `((function authorize!) (arguments ((user ,owner)))) 10) #f)
  (assert (authorize? admins '(*state* admin) `((function authorize!) (arguments ((user ,owner)))) 10) #t)

  ;; set-batch! remains owner/admin-only; an empty no-op does not authorize an
  ;; otherwise unrelated principal.
  (assert (authorize? admins local-peer '((function set-batch!) (arguments ((paths ((*state* alice docs)))))) 10) #f)
  (assert (authorize? admins owner '((function set-batch!) (arguments ((paths ((*state* alice docs)))))) 10) #t)
  (assert (authorize? admins local-peer
           '((function set-batch!) (arguments ((paths ()) (values ())))) 10)
          #f)

  #t)
